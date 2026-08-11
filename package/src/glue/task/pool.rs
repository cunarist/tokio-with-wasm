use crate::only_web::PATH_PROVIDER;
use crate::{BLOCKING_KEY, LogError, now};
use js_sys::{Array, JsString, Object, Reflect, global};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::{Closure, JsCast, JsValue, wasm_bindgen};
use wasm_bindgen::{memory, module};
use web_sys::{
  Blob, BlobPropertyBag, DedicatedWorkerGlobalScope, ErrorEvent, Event,
  MessageEvent, Url, Worker, WorkerOptions, WorkerType,
};

pub static MAX_WORKERS: usize = 512;

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
  /// Script path and the object URL of the bootstrap script built from it.
  script_url: RefCell<Option<(String, String)>>,
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
        script_url: RefCell::new(None),
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
  /// content security policy forbids `blob:` workers or `eval`, in which case
  /// a custom path provider is needed.
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
    let url = self.script_url()?;
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(&url, &options)?;

    // With a worker spun up send it the module/memory so it can start
    // instantiating the wasm module. Later it might receive further
    // messages about code to run on the wasm module.
    let worker_init = Object::new();
    Reflect::set(&worker_init, &JsString::from("module_or_path"), &module())?;
    Reflect::set(&worker_init, &JsString::from("memory"), &memory())?;
    worker.post_message(&worker_init)?;

    Ok(worker)
  }

  /// Returns the object URL of the bootstrap script for a worker.
  ///
  /// Every worker on this thread runs the same script, so the URL is built
  /// once and reused. Creating one URL per worker would leak an object URL
  /// for the whole lifetime of the page, and revoking it right away would
  /// race with the worker's script fetch.
  fn script_url(&self) -> Result<String, JsValue> {
    let script_path = PATH_PROVIDER.with(|provider| provider.borrow()())?;
    let mut cached = self.pool_state.script_url.borrow_mut();
    if let Some((cached_path, cached_url)) = cached.as_ref() {
      if *cached_path == script_path {
        return Ok(cached_url.clone());
      }
    }
    let url = Self::create_script_url(&script_path)?;
    *cached = Some((script_path, url.clone()));
    Ok(url)
  }

  fn create_script_url(script_path: &str) -> Result<String, JsValue> {
    let script = format!(
      "
      import init, * as wasmBindings from '{}';
      globalThis.wasmBindings = wasmBindings;
      globalThis.{BLOCKING_KEY} = true;
      self.onmessage = event => {{
        let initialised = init(event.data).catch(err => {{
          // Propagate to main `onerror`:
          setTimeout(() => {{
            throw err;
          }});
          // Rethrow to keep promise rejected
          // and prevent execution of further commands:
          throw err;
        }});

        self.onmessage = async event => {{
          // This will queue further commands up
          // until the module is fully initialised:
          await initialised;
          wasmBindings.task_worker_entry_point(event.data);
        }};
      }};
      ",
      script_path
    );
    let blob_property_bag = BlobPropertyBag::new();
    blob_property_bag.set_type("text/javascript");
    let blob = Blob::new_with_blob_sequence_and_options(
      &Array::from_iter([JsValue::from(script)]).into(),
      &blob_property_bag,
    )?;
    Url::create_object_url_with_blob(&blob)
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
    match worker.post_message(&JsValue::from(ptr as u32)) {
      Ok(()) => Ok(worker),
      Err(error) => {
        unsafe {
          drop(Box::from_raw(ptr));
        }
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
      if let Some(error) = event.dyn_ref::<ErrorEvent>() {
        JsValue::from_str(&error.message()).log_error("RECLAIM_EVENT");
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
        return;
      }

      // If this is a completion event then can deallocate our own
      // callback by clearing out `slot2` which contains our own closure.
      if let Some(_msg) = event.dyn_ref::<MessageEvent>() {
        if let Some(pool_state) = pool_state.upgrade() {
          pool_state.push_worker(worker2.clone());
        }
        *slot2.borrow_mut() = None;
        return;
      }

      // Unhandled worker event exists.
      JsValue::from_str(&format!("{event:?}")).log_error("UNHANDLED_RECLAIM");
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
      if *self.pool_state.total_workers_count.borrow() >= MAX_WORKERS {
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
pub fn task_worker_entry_point(ptr: u32) -> Result<(), JsValue> {
  let ptr = unsafe { Box::from_raw(ptr as *mut Task) };
  let global = global().unchecked_into::<DedicatedWorkerGlobalScope>();
  (ptr.callable)();
  global.post_message(&JsValue::undefined())?;
  Ok(())
}
