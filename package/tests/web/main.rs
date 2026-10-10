use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod macros_join;
mod macros_main;
mod macros_pin;
mod macros_select;
mod macros_try_join;
mod rt_common;
mod sync_broadcast;
mod sync_mpsc;
mod sync_oneshot;
mod sync_semaphore;
mod sync_semaphore_owned;
mod sync_watch;
mod task_abort;
mod task_blocking;
mod task_join_set;
mod task_yield_now;
mod time_interval;
mod time_sleep;
mod time_timeout;

/// Asserts that `ms` milliseconds have passed since `start`, allowing for
/// the rounding of `Date.now`.
fn assert_elapsed(start: f64, ms: u64) {
  let elapsed = js_sys::Date::now() - start;
  assert!(
    elapsed + 1.0 >= ms as f64,
    "only {elapsed}ms of {ms}ms passed"
  );
}
