use super::WORKER_POOL;
use crate::only_web::{PATH_PROVIDER, WORKER_SCRIPT_PROVIDER};
use crate::{log_error, now};
use js_sys::{Object, Reflect, global};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use wasm_bindgen::prelude::{Closure, JsCast, JsValue, wasm_bindgen};
use wasm_bindgen::{memory, module};
use web_sys::{
  DedicatedWorkerGlobalScope, ErrorEvent, Event, Worker, WorkerOptions,
  WorkerType,
};

#[cfg(not(test))]
pub const MAX_WORKERS: usize = 512;
/// Tests cap the pool at two workers to exercise saturation cheaply.
#[cfg(test)]
pub const MAX_WORKERS: usize = 2;

/// Boxed once more when sent, so that a thin pointer to it fits in a message.
type Task = Box<dyn FnOnce() + Send>;
/// Reports a task's failure, told whether it was handed to a worker.
type OnFailure = Box<dyn FnOnce(bool)>;

pub struct WorkerPool {
  /// Workers that count against `MAX_WORKERS`, idle or busy.
  workers_count: Cell<usize>,
  /// Idle workers with the time they turned idle.
  idle_workers: RefCell<Vec<(Worker, f64)>>,
  queued_tasks: RefCell<VecDeque<(Task, OnFailure)>>,
  /// Logs events from idle workers.
  callback: Closure<dyn FnMut(Event)>,
  /// Whether the periodic management task is running.
  is_managed: Cell<bool>,
}

impl WorkerPool {
  pub fn new() -> Self {
    WorkerPool {
      workers_count: Cell::new(0),
      idle_workers: RefCell::new(Vec::new()),
      queued_tasks: RefCell::new(VecDeque::new()),
      callback: Closure::new(|event: Event| log_error("POOL_CALLBACK", event)),
      is_managed: Cell::new(false),
    }
  }

  pub fn queue_task(
    &self,
    callable: impl FnOnce() + Send + 'static,
    on_failure: impl FnOnce(bool) + 'static,
  ) {
    self
      .queued_tasks
      .borrow_mut()
      .push_back((Box::new(callable), Box::new(on_failure)));
    self.flush_queued_tasks();
  }

  /// Hands queued tasks to idle workers, or to new ones while there is room.
  pub fn flush_queued_tasks(&self) {
    while !self.idle_workers.borrow().is_empty()
      || self.workers_count.get() < MAX_WORKERS
    {
      let Some((task, on_failure)) = self.queued_tasks.borrow_mut().pop_front()
      else {
        break;
      };
      match self.send(task) {
        Ok(worker) => reclaim_on_reply(worker, on_failure),
        Err(error) => {
          log_error("RUN_TASK", error);
          on_failure(false);
        }
      }
    }
  }

  /// Posts a task to an idle worker, or to a new one.
  fn send(&self, task: Task) -> Result<Worker, JsValue> {
    let idle_worker = self.idle_workers.borrow_mut().pop();
    let worker = match idle_worker {
      Some((worker, _)) => worker,
      None => {
        let url = WORKER_SCRIPT_PROVIDER.get()()?;
        // What the worker needs to instantiate the module.
        let init = Object::new();
        let glue_path = PATH_PROVIDER.get()()?.into();
        Reflect::set(&init, &"glue_path".into(), &glue_path)?;
        Reflect::set(&init, &"module_or_path".into(), &module())?;
        Reflect::set(&init, &"memory".into(), &memory())?;
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker =
          Worker::new_with_options(&url, &options).map_err(|error| {
            format!(
              "Creating a web worker from `{url}` failed with {error:?}. \
               If a content security policy forbids this URL, provide a \
               script file with \
               `tokio_with_wasm::only_web::set_worker_script_provider`."
            )
          })?;
        if let Err(error) = worker.post_message(&init) {
          worker.terminate();
          return Err(error);
        }
        self.workers_count.set(self.workers_count.get() + 1);
        worker
      }
    };
    let ptr = Box::into_raw(Box::new(task));
    // An `f64` crosses JS the same way with every `wasm-bindgen` version.
    if let Err(error) = worker.post_message(&(ptr as usize as f64).into()) {
      // Safety: the message never left, so the box is still ours.
      drop(unsafe { Box::from_raw(ptr) });
      worker.terminate();
      self.workers_count.set(self.workers_count.get() - 1);
      return Err(error);
    }
    Ok(worker)
  }

  /// Lets go of workers that have been idle for ten seconds.
  pub fn remove_inactive_workers(&self) {
    let current_time = now();
    self
      .idle_workers
      .borrow_mut()
      .retain(|(worker, idle_since)| {
        let is_active = current_time - idle_since < 10_000.0;
        if !is_active {
          // `null` asks the worker to free its stack in the shared memory
          // and close itself, which `terminate` would not do.
          if worker.post_message(&JsValue::NULL).is_err() {
            worker.terminate();
          }
          self.workers_count.set(self.workers_count.get() - 1);
        }
        is_active
      });
  }

  /// Reports whether the periodic management task still has something to do.
  /// Called by the management task itself, which stops when this is `false`.
  pub fn keep_managing(&self) -> bool {
    // Tasks only queue up while every worker is busy.
    let is_needed = self.workers_count.get() > 0;
    // Nothing can arrive before the caller returns, as nothing yields.
    self.is_managed.set(is_needed);
    is_needed
  }

  /// Returns whether the periodic management task has to be started.
  pub fn needs_managing(&self) -> bool {
    !self.is_managed.replace(true)
  }
}

/// Waits for the worker's single reply to its task. A message means the
/// task finished and the worker can take the next one. An error means the
/// task panicked or the script failed to load, so the worker is let go.
fn reclaim_on_reply(worker: Worker, on_failure: OnFailure) {
  let handler = Closure::once_into_js({
    let worker = worker.clone();
    move |event: Event| {
      WORKER_POOL.with(|pool| {
        if event.type_() == "message" {
          worker.set_onmessage(Some(pool.callback.as_ref().unchecked_ref()));
          worker.set_onerror(Some(pool.callback.as_ref().unchecked_ref()));
          pool.idle_workers.borrow_mut().push((worker, now()));
        } else {
          let reason = match event.dyn_ref::<ErrorEvent>() {
            Some(error) => error.message(),
            None => format!("worker event `{}`", event.type_()),
          };
          log_error("RECLAIM_EVENT", reason);
          worker.terminate();
          pool.workers_count.set(pool.workers_count.get() - 1);
          on_failure(true);
        }
        pool.flush_queued_tasks();
      });
    }
  });
  worker.set_onmessage(Some(handler.unchecked_ref()));
  worker.set_onerror(Some(handler.unchecked_ref()));
}

/// Entry point invoked by JavaScript in a worker, with the pointer that
/// [`WorkerPool::send`] posted. Exported only for the worker script.
#[wasm_bindgen]
pub fn task_worker_entry_point(ptr: f64) -> Result<(), JsValue> {
  let global = global().unchecked_into::<DedicatedWorkerGlobalScope>();
  // A worker script copied from 0.10 passes on the `null`
  // that closes the worker, which arrives here as zero.
  if ptr == 0.0 {
    global.close();
    return Ok(());
  }
  // Safety: each pointer is posted to a single worker exactly once.
  let task = unsafe { Box::from_raw(ptr as usize as *mut Task) };
  task();
  global.post_message(&JsValue::undefined())
}

#[cfg(test)]
mod tests {
  use super::MAX_WORKERS;
  use crate::now;
  use crate::task::{JoinError, spawn_blocking};
  use std::time::Duration;
  use wasm_bindgen_test::wasm_bindgen_test;

  /// Queued tasks go to workers as they turn idle,
  /// not on the next tick of the management timer.
  #[wasm_bindgen_test]
  async fn queued_tasks_go_to_workers_that_turn_idle() -> Result<(), JoinError>
  {
    let started = now();
    let handles: Vec<_> = (0..MAX_WORKERS * 25)
      .map(|task_index| spawn_blocking(move || task_index))
      .collect();
    for (task_index, handle) in handles.into_iter().enumerate() {
      assert_eq!(handle.await?, task_index);
    }
    let elapsed = now() - started;
    assert!(
      elapsed < 1_000.0,
      "queued tasks waited for ticks: {elapsed}ms"
    );
    Ok(())
  }

  #[wasm_bindgen_test]
  async fn abort_before_start_cancels_a_queued_task() {
    let busy: Vec<_> = (0..MAX_WORKERS)
      .map(|_| {
        spawn_blocking(|| std::thread::sleep(Duration::from_millis(200)))
      })
      .collect();
    let queued = spawn_blocking(|| 5);
    queued.abort();
    assert!(queued.await.is_err_and(|error| error.is_cancelled()));
    for handle in busy {
      assert!(handle.await.is_ok());
    }
  }
}
