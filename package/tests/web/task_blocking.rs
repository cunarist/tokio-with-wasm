use std::thread;
use std::time::Duration;
use tokio::task;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::DedicatedWorkerGlobalScope;

#[wasm_bindgen_test]
async fn basic_blocking() {
  for _ in 0..100 {
    let out = tokio::spawn(async {
      task::spawn_blocking(|| {
        thread::sleep(Duration::from_millis(5));
        "hello"
      })
      .await
      .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out, "hello");
  }
}

#[wasm_bindgen_test]
async fn runs_in_a_web_worker() {
  let in_worker = task::spawn_blocking(|| {
    js_sys::global().is_instance_of::<DedicatedWorkerGlobalScope>()
  });
  assert!(in_worker.await.unwrap());
}
