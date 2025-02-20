use crate::{print_fit, Dropper};
use chrono::Utc;
use std::time::Duration;
use tokio::task::{spawn, spawn_blocking, yield_now, JoinSet};
use tokio::time::{interval, sleep};
use tokio_with_wasm::alias as tokio;

#[tokio::main(flavor = "current_thread")]
pub async fn async_main() {
    test_join_handles().await;
    test_yield().await;
    test_interval().await;
    test_join_set().await;
}

async fn test_join_handles() {
    print_fit!("Tasks spawned");
    let async_join_handle = spawn(async {
        // Simulate a 2-second async task.
        sleep(Duration::from_secs(2)).await;
    });
    let blocking_join_handle = spawn_blocking(|| {
        // Simulate a 3-second blocking task.
        std::thread::sleep(Duration::from_secs(3));
    });

    let _async_result = async_join_handle.await;
    print_fit!("Async task joined");
    let _blocking_result = blocking_join_handle.await;
    print_fit!("Blocking task joined");
}

async fn test_yield() {
    for i in 1..=500 {
        yield_now().await;
        // Run some code that blocks for a few milliseconds.
        calculate_cpu_bound();
        if i % 100 == 0 {
            print_fit!("Repeating task, iteration: {}", i);
        }
    }
}

async fn test_interval() {
    let mut ticker = interval(Duration::from_secs(1));
    for i in 1..=5 {
        ticker.tick().await;
        print_fit!("Interval task, iteration: {}", i);
    }
}

fn calculate_cpu_bound() {
    let start = Utc::now().timestamp_millis();
    let mut _sum = 0.0;
    while Utc::now().timestamp_millis() - start < 10 {
        for i in 0..10_000 {
            _sum += (i as f64).sqrt().sin().cos();
        }
    }
}

async fn test_join_set() {
    print_fit!("Creating JoinSet");
    let mut join_set: JoinSet<_> = JoinSet::new();
    // Spawn many async tasks and add them to the JoinSet.
    for i in 1..=4 {
        join_set.spawn(async move {
            let _dropper = Dropper {
                name: format!("FROM_JOIN_SET_{}", i),
            };
            tokio::time::sleep(Duration::from_secs(i as u64)).await;
        });
    }
    // Await only some of the tasks in the JoinSet.
    // Unfinished tasks should be aborted when the join set is dropped.
    for _ in 1..=2 {
        if let Some(result) = join_set.join_next().await {
            match result {
                Ok(_) => print_fit!("A JoinSet task finished successfully"),
                Err(_) => print_fit!("A JoinSet task encountered an error"),
            }
        }
    }
    print_fit!("Dropping JoinSet");
}
