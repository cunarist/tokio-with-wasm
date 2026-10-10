use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::SetOnce;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[derive(Clone)]
struct DropCounter {
  drops: Arc<AtomicU32>,
}

impl DropCounter {
  fn new() -> Self {
    DropCounter {
      drops: Arc::new(AtomicU32::new(0)),
    }
  }

  fn assert_num_drops(&self, value: u32) {
    assert_eq!(value, self.drops.load(Ordering::Relaxed));
  }
}

impl Drop for DropCounter {
  fn drop(&mut self) {
    self.drops.fetch_add(1, Ordering::Relaxed);
  }
}

#[wasm_bindgen_test]
fn drop_cell() {
  let fooer = DropCounter::new();
  {
    let once_cell = SetOnce::new();
    assert!(once_cell.set(fooer.clone()).is_ok());
  }
  fooer.assert_num_drops(1);
}

#[wasm_bindgen_test]
fn drop_cell_new_with() {
  let fooer = DropCounter::new();
  {
    let once_cell = SetOnce::new_with(Some(fooer.clone()));
    assert!(once_cell.initialized());
  }
  fooer.assert_num_drops(1);
}

#[wasm_bindgen_test]
fn drop_into_inner() {
  let fooer = DropCounter::new();
  let once_cell = SetOnce::new();
  assert!(once_cell.set(fooer.clone()).is_ok());
  let val = once_cell.into_inner();
  fooer.assert_num_drops(0);
  drop(val);
  fooer.assert_num_drops(1);
}

#[wasm_bindgen_test]
fn drop_into_inner_new_with() {
  let fooer = DropCounter::new();
  let once_cell = SetOnce::new_with(Some(fooer.clone()));
  let val = once_cell.into_inner();
  fooer.assert_num_drops(0);
  drop(val);
  fooer.assert_num_drops(1);
}

#[wasm_bindgen_test]
fn from() {
  let cell = SetOnce::from(2);
  assert_eq!(*cell.get().unwrap(), 2);
}

#[wasm_bindgen_test]
fn set_and_get() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();

  ONCE.set(5).unwrap();
  assert_eq!(*ONCE.get().unwrap(), 5);
}

#[wasm_bindgen_test]
async fn set_and_wait() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();

  tokio::spawn(async { ONCE.set(5) });

  assert_eq!(*ONCE.wait().await, 5);
}

#[wasm_bindgen_test]
async fn set_and_wait_from_a_web_worker() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();

  tokio::task::spawn_blocking(|| ONCE.set(4).unwrap());

  assert_eq!(*ONCE.wait().await, 4);
}

#[wasm_bindgen_test]
async fn set_from_two_web_workers() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();

  let a = tokio::task::spawn_blocking(|| ONCE.set(4).is_err());
  let b = tokio::task::spawn_blocking(|| ONCE.set(3).is_err());

  assert!(a.await.unwrap() != b.await.unwrap());
}

#[wasm_bindgen_test]
fn get_uninit() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();
  assert!(ONCE.get().is_none());
}

#[wasm_bindgen_test]
fn set_twice() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();

  assert_eq!(ONCE.set(5), Ok(()));
  assert!(ONCE.set(6).is_err());
}

#[wasm_bindgen_test]
fn is_none_initializing() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();

  assert_eq!(ONCE.get(), None);
  ONCE.set(20).unwrap();
  assert!(ONCE.set(10).is_err());
}

#[wasm_bindgen_test]
async fn is_some_initializing() {
  static ONCE: SetOnce<u32> = SetOnce::const_new();

  tokio::spawn(async { ONCE.set(20) });

  assert_eq!(*ONCE.wait().await, 20);
}

#[wasm_bindgen_test]
fn into_inner_int_empty_setonce() {
  assert!(SetOnce::<u32>::new().into_inner().is_none());
}
