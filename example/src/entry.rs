use crate::print_fit;
use chrono::Utc;
use std::time::Duration;
use tokio::task::{spawn, spawn_blocking, yield_now};
use tokio::time::{interval, sleep};
use tokio_with_wasm::alias as tokio;

#[tokio::main(flavor = "current_thread")]
pub async fn async_main() {
    print_fit!("Tasks spawned.");
    let async_join_handle = spawn(async {
        // Asynchronous code here.
        // This will run concurrently
        // in the same web worker(thread).
        // Simulate a 2-second async task.
        sleep(Duration::from_secs(2)).await;
    });
    let blocking_join_handle = spawn_blocking(|| {
        // Blocking code here.
        // This will run parallelly
        // in the external pool of web workers.
        // Simulate a 3-second blocking task.
        std::thread::sleep(Duration::from_secs(3));
    });

    let _async_result = async_join_handle.await;
    print_fit!("Async task joined.");
    let _blocking_result = blocking_join_handle.await;
    print_fit!("Blocking task joined.");

    for i in 1..=1000 {
        // Some repeating task here
        // that shouldn't block the JavaScript runtime.
        yield_now().await;
        calculate_cpu_bound();
        if i % 100 == 0 {
            print_fit!("Repeating task, iteration: {}", i);
        }
    }

    test_interval().await;
}

async fn test_interval() {
    let mut ticker = interval(Duration::from_secs(1));
    for i in 1..=5 {
        ticker.tick().await;
        yield_now().await;
        print_fit!("Interval task, iteration: {}", i);
    }
}

fn calculate_cpu_bound() {
    let start = Utc::now().timestamp_millis();
    let mut _sum = 0.0;
    while Utc::now().timestamp_millis() - start < 5 {
        for i in 0..10_000 {
            _sum += (i as f64).sqrt().sin().cos();
        }
    }
}
