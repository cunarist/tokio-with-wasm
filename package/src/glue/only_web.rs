//! Functions specific to WebAssembly web targets.
//! These functions are only available when compiling for `wasm32` arch.

use wasm_bindgen::JsValue;
use crate::WORKER_POOL;

/// Sets a custom path provider for the worker scripts.
/// This function should be called before spawning any tasks.
/// The provider function should return the URL or path to the worker script.
/// # Example
/// ```rust,no_run
/// use tokio_with_wasm::only_web::set_path_provider;
/// use wasm_bindgen::JsValue;
/// 
/// set_path_provider(|| {
///     Ok(String::from("/custom/path/to/worker.js"))
/// });
/// ```
#[inline(always)]
pub fn set_path_provider(provider: fn() -> Result<String, JsValue>) {
    WORKER_POOL.with(|pool| {
        pool.set_path_provider(provider);
    });
}