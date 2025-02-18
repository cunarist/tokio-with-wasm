use crate::print_fit;
use std::time::Duration;
use tokio::task::{spawn, spawn_blocking, yield_now};
use tokio::time::sleep;
use tokio_with_wasm::alias as tokio;

#[tokio::main(flavor = "current_thread")]
pub async fn async_main() {
    let async_join_handle = spawn(async {
        // Asynchronous code here.
        // This will run concurrently
        // in the same web worker(thread).
        print_fit!("Async task started.");
        // Simulate a 3-second async task.
        sleep(Duration::from_secs(3)).await;
        print_fit!("Async task finished.");
    });

    let blocking_join_handle = spawn_blocking(|| {
        // Blocking code here.
        // This will run parallelly
        // in the external pool of web workers.
        print_fit!("Blocking task started.");
        // Simulate a 3-second blocking task.
        std::thread::sleep(Duration::from_secs(3));
        print_fit!("Blocking task finished.");
    });

    let _async_result = async_join_handle.await;
    let _blocking_result = blocking_join_handle.await;

    for i in 1..1000 {
        // Some repeating task here
        // that shouldn't block the JavaScript runtime.
        yield_now().await;
        if i % 100 == 0 {
            print_fit!("Repeating task, iteration: {}", i);
            // Ensure it doesn't hog CPU by taking some breaks.
            sleep(Duration::from_millis(500)).await;
        }
    }
}
