//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.

/// Waits until `duration` has elapsed.
///
/// Because this is a naive implemenation
/// based on `setTimeout()` of JavaScript,
/// web browsers might increase the interval arbitrarily
/// to save system resources.
pub async fn sleep(duration: std::time::Duration) {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = setTimeout)]
        fn set_timeout(callback: &js_sys::Function, milliseconds: f64);
    }
    let milliseconds = duration.as_millis() as f64;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        set_timeout(&resolve, milliseconds);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
