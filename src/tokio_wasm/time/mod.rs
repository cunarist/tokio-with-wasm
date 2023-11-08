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
