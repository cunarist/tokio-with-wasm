use crate::only_web::{PATH_PROVIDER, WORKER_SCRIPT_PROVIDER};
use crate::{LogError, now};
use js_sys::{JsString, Object, Reflect, global};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::{Closure, JsCast, JsValue, wasm_bindgen};
use wasm_bindgen::{memory, module};
use web_sys::{
  DedicatedWorkerGlobalScope, ErrorEvent, Event, MessageEvent, Worker,
  WorkerOptions, WorkerType,
};

#[cfg(not(test))]
pub const MAX_WORKERS: usize = 512;
/// Tests cap the pool at two workers,
/// so that saturation can be exercised without creating hundreds of them.
#[cfg(test)]
pub const MAX_WORKERS: usize = 2;

pub struct WorkerPool {
  pool_state: Rc<PoolState>,
}

struct PoolState {
  /// Number of workers that count against `MAX_WORKERS`,
  /// whether they are idle or busy.
  total_workers_count: RefCell<usize>,
  idle_workers: RefCell<Vec<ManagedWorker>>,
  queued_tasks: RefCell<VecDeque<QueuedTask>>,
  callback: Closure<dyn FnMut(Event)>,
  /// Whether the periodic management task is currently running.
  is_managed: Cell<bool>,
}

struct ManagedWorker {
  deactivated_time: RefCell<f64>, // Timestamp in milliseconds
  worker: Worker,
}

/// A unit of work that is sent to a web worker.
struct Task {
  callable: Box<dyn FnOnce() + Send>,
}

/// A task waiting for a web worker, together with the handler that reports
/// the failure if the task never gets to run or dies halfway through.
/// The handler stays on this thread, so it doesn't have to be `Send`.
struct QueuedTask {
  task: Task,
  on_failure: Box<dyn FnOnce()>,
}

impl Default for WorkerPool {
  fn default() -> Self {
    WorkerPool {
      pool_state: Rc::new(PoolState {
        total_workers_count: RefCell::new(0),
        idle_workers: RefCell::new(Vec::with_capacity(MAX_WORKERS)),
        queued_tasks: RefCell::new(VecDeque::new()),
        callback: Closure::new(|event: Event| {
          JsValue::from_str(&format!("{event:?}")).log_error("POOL_CALLBACK");
        }),
        is_managed: Cell::new(false),
      }),
    }
  }
}

impl WorkerPool {
  /// Creates a new empty `WorkerPool`.
  ///
  /// Workers are created on demand as tasks arrive, and are terminated
  /// again once they have been idle for a while.
  pub fn new() -> WorkerPool {
    WorkerPool::default()
  }

  /// Unconditionally spawns a new worker.
  ///
  /// The worker isn't registered with this `WorkerPool` but is capable of
  /// executing work for this wasm module.
  ///
  /// # Errors
  ///
  /// Returns any error that may happen while a JS web worker is created and a
  /// message is sent to it. Creating a worker fails when the document's
  /// content security policy forbids the `blob:` URL that the worker script
  /// is served from by default, in which case a custom worker script
  /// provider is needed.
  fn create_worker(&self) -> Result<Worker, JsValue> {
    *self.pool_state.total_workers_count.borrow_mut() += 1;
    let created = self.create_worker_inner();
    if created.is_err() {
      // The worker will never exist, so it must not take up a slot.
      self.pool_state.discard_worker();
    }
    created
  }

  fn create_worker_inner(&self) -> Result<Worker, JsValue> {
    // The provider fns are copied out of their cells before the calls,
    // so that a provider which sets a provider itself doesn't hit a
    // double borrow.
    let script_provider = WORKER_SCRIPT_PROVIDER.with(|p| *p.borrow());
    let url = script_provider()?;
    let path_provider = PATH_PROVIDER.with(|p| *p.borrow());
    let glue_path = path_provider()?;
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(&url, &options).map_err(|error| {
      // A content security policy that forbids the worker's URL, such as
      // a browser extension's `script-src 'self'` with the default
      // `blob:` URL, lands here as a bare `SecurityError`.
      JsValue::from_str(&format!(
        "Creating a web worker from `{url}` failed with {error:?}. \
           If a content security policy forbids this URL, provide a \
           script file with \
           `tokio_with_wasm::only_web::set_worker_script_provider`."
      ))
    })?;

    // With a worker spun up send it the glue path and the module/memory so
    // it can start instantiating the wasm module. Later it might receive
    // further messages about code to run on the wasm module.
    let worker_init = Object::new();
    Reflect::set(
      &worker_init,
      &JsString::from("glue_path"),
      &JsValue::from(glue_path),
    )?;
    Reflect::set(&worker_init, &JsString::from("module_or_path"), &module())?;
    Reflect::set(&worker_init, &JsString::from("memory"), &memory())?;
    worker.post_message(&worker_init)?;

    Ok(worker)
  }

  /// Fetches a worker from this pool, creating one if necessary.
  ///
  /// This will attempt to pull an already-spawned web worker from our cache
  /// if one is available, otherwise it will spawn a new worker and return the
  /// newly spawned worker.
  ///
  /// # Errors
  ///
  /// Returns any error that may happen while a JS web worker is created and a
  /// message is sent to it.
  fn get_worker(&self) -> Result<Worker, JsValue> {
    match self.pool_state.idle_workers.borrow_mut().pop() {
      Some(managed_worker) => Ok(managed_worker.worker),
      None => self.create_worker(),
    }
  }

  /// Executes the work `f` in a web worker, spawning a web worker if
  /// necessary.
  ///
  /// This will acquire a web worker and then send the closure `f` to the
  /// worker to execute. The worker won't be usable for anything else while
  /// `f` is executing, and no callbacks are registered for when the worker
  /// finishes.
  ///
  /// # Errors
  ///
  /// Returns any error that may happen while a JS web worker is created and a
  /// message is sent to it.
  fn execute(&self, task: Task) -> Result<Worker, JsValue> {
    let worker = self.get_worker()?;
    let work = Box::new(task);
    let ptr = Box::into_raw(work);
    // `usize`, not `u32`, so that the pointer survives on `wasm64`.
    // It crosses JS as an `f64` there, which is exact below 2^53.
    match worker.post_message(&JsValue::from(ptr as usize)) {
      Ok(()) => Ok(worker),
      Err(error) => {
        unsafe {
          drop(Box::from_raw(ptr));
        }
        // The worker never received the task, so it cannot be trusted
        // with another one. It is terminated and gives up its slot;
        // keeping it would let repeated failures fill the pool
        // with unusable workers and stall the queue.
        worker.terminate();
        self.pool_state.discard_worker();
        Err(error)
      }
    }
  }

  /// Configures the callbacks for the `worker` specified for the
  /// web worker to be reclaimed and re-inserted into this pool when a message
  /// is received.
  ///
  /// Currently this `WorkerPool` abstraction is intended to execute one-off
  /// style work where the work itself doesn't send any notifications and
  /// when it's done the worker is ready to execute more work. This method is
  /// used for all spawned workers to ensure that when the work is finished
  /// the worker is reclaimed back into this pool.
  ///
  /// A worker that reports an error never sends its completion message,
  /// which happens when the task inside it panics. It is dropped from the
  /// pool and `on_failure` is called, so that the task's `JoinHandle` gets
  /// an answer instead of waiting forever.
  fn reclaim_on_message(&self, worker: Worker, on_failure: Box<dyn FnOnce()>) {
    let pool_state = Rc::downgrade(&self.pool_state);
    let worker2 = worker.clone();
    let reclaim_slot = Rc::new(RefCell::new(None));
    let slot2 = reclaim_slot.clone();
    let on_failure = RefCell::new(Some(on_failure));
    let reclaim = Closure::<dyn FnMut(_)>::new(move |event: Event| {
      // A completion message reclaims the worker into the pool,
      // and deallocates this callback by clearing out `slot2`,
      // which contains this closure itself.
      let is_completion = event
        .dyn_ref::<MessageEvent>()
        .is_some_and(|message| message.type_() == "message");
      if is_completion {
        if let Some(pool_state) = pool_state.upgrade() {
          pool_state.push_worker(worker2.clone());
        }
        *slot2.borrow_mut() = None;
        return;
      }

      // Anything else means the worker is unusable: an `ErrorEvent`
      // from a panicking task, or the plain `Event` of type `error`
      // that fires when the worker's script fails to load.
      let reason = match event.dyn_ref::<ErrorEvent>() {
        Some(error) => error.message(),
        None => format!("worker event `{}`", event.type_()),
      };
      JsValue::from_str(&reason).log_error("RECLAIM_EVENT");
      // The worker's memory is left in an unknown state,
      // so it is terminated instead of being reused.
      worker2.terminate();
      if let Some(pool_state) = pool_state.upgrade() {
        pool_state.discard_worker();
      }
      if let Some(on_failure) = on_failure.borrow_mut().take() {
        on_failure();
      }
      *slot2.borrow_mut() = None;
    });
    worker.set_onmessage(Some(reclaim.as_ref().unchecked_ref()));
    worker.set_onerror(Some(reclaim.as_ref().unchecked_ref()));
    *reclaim_slot.borrow_mut() = Some(reclaim);
  }
}

impl WorkerPool {
  /// Executes `f` in a web worker.
  ///
  /// This pool manages a set of web workers to draw from, and `f` will be
  /// spawned quickly into one if the worker is idle. If no idle workers are
  /// available then a new web worker will be spawned.
  ///
  /// Once the task returns, the worker assigned to it is automatically
  /// reclaimed by this `WorkerPool`.
  ///
  /// If the task cannot be handed to a worker at all, its failure handler is
  /// called right away. The handler is also kept for the case where the
  /// worker dies while running the task.
  fn run(&self, queued_task: QueuedTask) {
    let QueuedTask { task, on_failure } = queued_task;
    let worker = match self.execute(task) {
      Ok(worker) => worker,
      Err(error) => {
        error.log_error("RUN_TASK");
        on_failure();
        return;
      }
    };
    self.reclaim_on_message(worker, on_failure);
  }

  pub fn remove_inactive_workers(&self) {
    let mut idle_workers = self.pool_state.idle_workers.borrow_mut();
    let current_timestamp = now();
    idle_workers.retain(|managed_worker| {
      let deactivated_time = *managed_worker.deactivated_time.borrow();
      let passed_time = current_timestamp - deactivated_time;
      let is_active = passed_time < 10000.0; // 10 seconds
      if !is_active {
        managed_worker.worker.terminate();
        self.pool_state.discard_worker();
      }
      is_active
    });
  }

  pub fn flush_queued_tasks(&self) {
    loop {
      // A task can run when a worker is sitting idle, or when there is
      // room to create a new one. Checking only the total count would
      // strand queued tasks while the pool is at its limit: workers
      // returning to the idle list don't lower the count, so the queue
      // would wait for the ten-second cull instead of reusing them.
      let has_capacity = !self.pool_state.idle_workers.borrow().is_empty()
        || *self.pool_state.total_workers_count.borrow() < MAX_WORKERS;
      if !has_capacity {
        break;
      }
      // The queue is not borrowed while the task runs,
      // because a failing task can queue another one.
      let queued_task =
        match self.pool_state.queued_tasks.borrow_mut().pop_front() {
          Some(inner) => inner,
          None => break,
        };
      self.run(queued_task);
    }
  }

  /// Reports whether the periodic management task still has something to do.
  /// Called by the management task itself, which stops when this is `false`.
  pub fn keep_managing(&self) -> bool {
    let is_needed = *self.pool_state.total_workers_count.borrow() > 0
      || !self.pool_state.queued_tasks.borrow().is_empty();
    if !is_needed {
      // Nothing can arrive between this and the caller's return,
      // because both run on the same thread without yielding.
      self.pool_state.is_managed.set(false);
    }
    is_needed
  }

  /// Returns whether the periodic management task has to be started.
  pub fn needs_managing(&self) -> bool {
    !self.pool_state.is_managed.replace(true)
  }

  pub fn queue_task(
    &self,
    callable: impl FnOnce() + Send + 'static,
    on_failure: impl FnOnce() + 'static,
  ) {
    let mut queued_tasks = self.pool_state.queued_tasks.borrow_mut();
    queued_tasks.push_back(QueuedTask {
      task: Task {
        callable: Box::new(callable),
      },
      on_failure: Box::new(on_failure),
    });
    drop(queued_tasks);
    self.flush_queued_tasks();
  }
}

impl PoolState {
  fn push_worker(&self, worker: Worker) {
    worker.set_onmessage(Some(self.callback.as_ref().unchecked_ref()));
    worker.set_onerror(Some(self.callback.as_ref().unchecked_ref()));
    let mut workers = self.idle_workers.borrow_mut();
    let is_known = workers.iter().any(|managed_worker| {
      let previous: &JsValue = &managed_worker.worker;
      let current: &JsValue = &worker;
      previous == current
    });
    if is_known {
      // A worker sent two completion messages for a single task.
      // Registering it twice would hand it to two tasks at once.
      JsValue::from_str("A web worker was reclaimed twice")
        .log_error("PUSH_WORKER");
      return;
    }
    workers.push(ManagedWorker {
      deactivated_time: RefCell::new(now()),
      worker,
    });
  }

  /// Gives up a worker slot, letting a queued task take it.
  fn discard_worker(&self) {
    let mut count = self.total_workers_count.borrow_mut();
    *count = count.saturating_sub(1);
  }
}

/// Entry point invoked by JavaScript in a worker.
#[wasm_bindgen]
pub fn task_worker_entry_point(ptr: usize) -> Result<(), JsValue> {
  let ptr = unsafe { Box::from_raw(ptr as *mut Task) };
  let global = global().unchecked_into::<DedicatedWorkerGlobalScope>();
  (ptr.callable)();
  global.post_message(&JsValue::undefined())?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::MAX_WORKERS;
  use crate::now;
  use crate::task::{JoinError, spawn_blocking};
  use wasm_bindgen_test::wasm_bindgen_test;

  /// Twice as many tasks as the pool may hold workers.
  /// The excess tasks must be handed to workers that turn idle,
  /// not wait for the ten-second cull to make room for new ones.
  #[wasm_bindgen_test]
  async fn queued_tasks_reuse_idle_workers_at_the_cap() -> Result<(), JoinError>
  {
    let started = now();
    let handles: Vec<_> = (0..MAX_WORKERS * 2)
      .map(|task_index| {
        spawn_blocking(move || {
          std::thread::sleep(std::time::Duration::from_millis(200));
          task_index
        })
      })
      .collect();
    for (task_index, handle) in handles.into_iter().enumerate() {
      assert_eq!(handle.await?, task_index);
    }
    let elapsed = now() - started;
    assert!(
      elapsed < 8_000.0,
      "the queued tasks waited for the idle-worker cull: {elapsed}ms"
    );
    Ok(())
  }
}
