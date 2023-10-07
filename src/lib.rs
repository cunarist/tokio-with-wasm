use futures_channel::oneshot;
pub use join_handle::*;
use pool::*;
use wasm_bindgen::prelude::*;

mod join_handle;
mod pool;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_js_value(x: &JsValue);
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn now() -> f64;
    #[wasm_bindgen(js_name = setTimeout)]
    fn set_timeout(callback: &js_sys::Function, milliseconds: f64);
}

macro_rules! console_log {
    ($($t:tt)*) => (crate::log(&format_args!($($t)*).to_string()))
}
pub(crate) use console_log;

thread_local! {
    pub(crate) static WORKER_POOL: WorkerPool = WorkerPool::new().unwrap();
}

/// Spawns a new asynchronous task, returning a
/// [`JoinHandle`] for it.
///
/// The provided future will start running in the background immediately
/// when `spawn` is called, even if you don't await the returned
/// `JoinHandle`.
///
/// Spawning a task enables the task to execute concurrently to other tasks. The
/// spawned task will always execute on the current thread,
/// as that's how JavaScript's `Promise` basically works.
///
/// # Examples
///
/// In this example, a server is started and `spawn` is used to start a new task
/// that processes each received connection.
///
/// ```no_run
/// use std::io;
///
/// async fn process() -> io::Result<()> {
///     // Some process...
/// }
///
/// async fn start() -> io::Result<()> {
///     let result = async_wasm_task::spawn(async move {
///         // Process this job concurrently.
///         process(socket).await
///     }).await?;;
/// }
/// ```
///
/// To run multiple tasks in parallel and receive their results, join
/// handles can be stored in a vector.
/// ```
/// # async fn start() {
/// async fn my_background_op(id: i32) -> String {
///     let s = format!("Starting background task {}.", id);
///     println!("{}", s);
///     s
/// }
///
/// let ops = vec![1, 2, 3];
/// let mut tasks = Vec::with_capacity(ops.len());
/// for op in ops {
///     // This call will make them start running in the background
///     // immediately.
///     tasks.push(async_wasm_task::spawn(my_background_op(op)));
/// }
///
/// let mut outputs = Vec::with_capacity(tasks.len());
/// for task in tasks {
///     outputs.push(task.await.unwrap());
/// }
/// println!("{:?}", outputs);
/// # }
/// ```
/// This example pushes the tasks to `outputs` in the order they were
/// started in.
///
/// # Using `!Send` values from a task
///
/// The task supplied to `spawn` is not required to implement `Send`.
/// This is different from multi-threaded native async runtimes,
/// because JavaScript environment is inherently single-threaded.
///
/// For example, this will work:
///
/// ```
/// use std::rc::Rc;
///
/// fn use_rc(rc: Rc<()>) {
///     // Do stuff w/ rc
/// # drop(rc);
/// }
///
/// async fn start() {
///     async_wasm_task::spawn(async {
///         // Force the `Rc` to stay in a scope with no `.await`
///         {
///             let rc = Rc::new(());
///             use_rc(rc.clone());
///         }
///
///         async_wasm_task::yield_now().await;
///     }).await.unwrap();
/// }
/// ```
///
/// This will work too, unlike multi-threaded native runtimes
/// where `!Send` values cannot live across `.await`:
///
/// ```
/// use std::rc::Rc;
///
/// fn use_rc(rc: Rc<()>) {
///     // Do stuff w/ rc
/// # drop(rc);
/// }
///]
/// async fn start() {
///     async_wasm_task::spawn(async {
///         let rc = Rc::new(());
///
///         async_wasm_task::yield_now().await;
///
///         use_rc(rc.clone());
///     }).await.unwrap();
/// }
/// ```
pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: std::future::Future<Output = T> + 'static,
    T: 'static,
{
    let (sender, receiver) = oneshot::channel();
    wasm_bindgen_futures::spawn_local(async move {
        let output = future.await;
        drop(sender.send(output));
    });
    JoinHandle::new(receiver)
}

/// Runs the provided closure on a web worker(thread) where blocking is acceptable.
///
/// In general, issuing a blocking call or performing a lot of compute in a
/// future without yielding is problematic, as it may prevent the JavaScript runtime from
/// driving other futures forward. This function runs the provided closure on a
/// web worker dedicated to blocking operations.
///
/// More and more web workers will be spawned when they are requested through this
/// function until the upper limit of 512 is reached.
/// After reaching the upper limit, the tasks will wait for
/// any of the web workers to become idle.
/// When a web worker remains idle for 10 seconds, it will be terminated
/// and get removed from the worker pool, which is a similiar behavior to that of `tokio`.
/// The web worker limit is very large by default, because `spawn_blocking` is often
/// used for various kinds of IO operations that cannot be performed
/// asynchronously.  When you run CPU-bound code using `spawn_blocking`, you
/// should keep this large upper limit in mind.
///
/// This function is intended for non-async operations that eventually finish on
/// their own. Because web workers do not share memory like threads do,
/// synchronization primitives such as mutex, channels, and global static variables
/// might not work as expected. Each web worker is completely isolated
/// by the web standards.
///
/// # Examples
///
/// Pass an input value and receive result of computation:
///
/// ```
/// # async fn docs() -> Result<(), Box<dyn std::error::Error>>{
/// // Initial input
/// let mut data = "Hello, ".to_string();
/// let output = async_wasm_task::spawn_blocking(move || {
///     // Stand-in for compute-heavy work or using synchronous APIs
///     data.push_str("world");
///     // Pass ownership of the value back to the asynchronous context
///     data
/// }).await?;
///
/// // `output` is the value returned from the thread
/// assert_eq!(output.as_str(), "Hello, world");
/// Ok(())
/// }
/// ```
pub fn spawn_blocking<C, T>(callable: C) -> JoinHandle<T>
where
    C: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    WORKER_POOL.with(|worker_pool| {
        worker_pool.queue_task(move || {
            let output = callable();
            drop(sender.send(output));
        })
    });
    start_managing_pool();
    JoinHandle::new(receiver)
}

/// Yields execution back to the JavaScript event loop.
///
/// To avoid blocking inside a long-running function,
/// you have to yield to the async event loop regularly.
///
/// The async task may resume when it has its turn back.
/// Meahwhile, any other pending tasks will be scheduled.
pub async fn yield_now() {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = queueMicrotask)]
        fn queue_microtask(callback: &js_sys::Function);
    }
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        queue_microtask(&resolve);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
