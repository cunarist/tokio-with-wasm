use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OnceCell, SetError};
use tokio::time;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

struct Foo {
  value: Arc<AtomicU32>,
}

impl Drop for Foo {
  fn drop(&mut self) {
    self.value.fetch_add(1, Ordering::Release);
  }
}

impl From<Arc<AtomicU32>> for Foo {
  fn from(value: Arc<AtomicU32>) -> Self {
    Foo { value }
  }
}

#[wasm_bindgen_test]
fn drop_cell() {
  let num_drops = Arc::new(AtomicU32::new(0));
  {
    let once_cell = OnceCell::new();
    assert!(once_cell.set(Foo::from(num_drops.clone())).is_ok());
  }
  assert_eq!(num_drops.load(Ordering::Acquire), 1);
}

#[wasm_bindgen_test]
fn drop_cell_new_with() {
  let num_drops = Arc::new(AtomicU32::new(0));
  {
    let once_cell = OnceCell::new_with(Some(Foo::from(num_drops.clone())));
    assert!(once_cell.initialized());
  }
  assert_eq!(num_drops.load(Ordering::Acquire), 1);
}

#[wasm_bindgen_test]
fn drop_into_inner() {
  let num_drops = Arc::new(AtomicU32::new(0));
  let once_cell = OnceCell::new();
  assert!(once_cell.set(Foo::from(num_drops.clone())).is_ok());
  let foo = once_cell.into_inner();
  assert_eq!(num_drops.load(Ordering::Acquire), 0);
  drop(foo);
  assert_eq!(num_drops.load(Ordering::Acquire), 1);
}

#[wasm_bindgen_test]
fn drop_into_inner_new_with() {
  let num_drops = Arc::new(AtomicU32::new(0));
  let once_cell = OnceCell::new_with(Some(Foo::from(num_drops.clone())));
  let foo = once_cell.into_inner();
  assert_eq!(num_drops.load(Ordering::Acquire), 0);
  drop(foo);
  assert_eq!(num_drops.load(Ordering::Acquire), 1);
}

#[wasm_bindgen_test]
fn from() {
  let cell = OnceCell::from(2);
  assert_eq!(*cell.get().unwrap(), 2);
}

async fn func1() -> u32 {
  5
}

async fn func2() -> u32 {
  time::sleep(Duration::from_millis(1)).await;
  10
}

async fn func_err() -> Result<u32, ()> {
  Err(())
}

async fn func_ok() -> Result<u32, ()> {
  Ok(10)
}

async fn sleep_and_set() -> u32 {
  time::sleep(Duration::from_millis(50)).await;
  5
}

async fn sleep_and_set_cell(
  cell: &'static OnceCell<u32>,
  v: u32,
) -> Result<(), SetError<u32>> {
  time::sleep(Duration::from_millis(1)).await;
  cell.set(v)
}

#[wasm_bindgen_test]
async fn get_or_init() {
  static ONCE: OnceCell<u32> = OnceCell::const_new();

  let handle1 = tokio::spawn(async { ONCE.get_or_init(func1).await });
  let handle2 = tokio::spawn(async { ONCE.get_or_init(func2).await });

  assert_eq!(*handle1.await.unwrap(), 5);
  assert_eq!(*handle2.await.unwrap(), 5);
}

#[wasm_bindgen_test]
async fn get_or_init_from_a_web_worker() {
  static ONCE: OnceCell<u32> = OnceCell::const_new();

  tokio::task::spawn_blocking(|| ONCE.set(7).unwrap())
    .await
    .unwrap();

  assert_eq!(*ONCE.get_or_init(func2).await, 7);
}

#[wasm_bindgen_test]
async fn set_and_get() {
  static ONCE: OnceCell<u32> = OnceCell::const_new();

  let _ = tokio::spawn(async { ONCE.set(5) }).await;
  assert_eq!(*ONCE.get().unwrap(), 5);
}

#[wasm_bindgen_test]
fn get_uninit() {
  static ONCE: OnceCell<u32> = OnceCell::const_new();
  assert!(ONCE.get().is_none());
}

#[wasm_bindgen_test]
fn set_twice() {
  static ONCE: OnceCell<u32> = OnceCell::const_new();

  assert_eq!(ONCE.set(5), Ok(()));
  assert!(ONCE.set(6).err().unwrap().is_already_init_err());
}

#[wasm_bindgen_test]
async fn set_while_initializing() {
  static ONCE: OnceCell<u32> = OnceCell::const_new();

  let handle1 = tokio::spawn(async { ONCE.get_or_init(sleep_and_set).await });
  let handle2 = tokio::spawn(sleep_and_set_cell(&ONCE, 10));

  assert_eq!(*handle1.await.unwrap(), 5);
  assert!(handle2.await.unwrap().err().unwrap().is_initializing_err());
}

#[wasm_bindgen_test]
async fn get_or_try_init() {
  static ONCE: OnceCell<u32> = OnceCell::const_new();

  let handle1 = tokio::spawn(async { ONCE.get_or_try_init(func_err).await });
  let handle2 = tokio::spawn(async { ONCE.get_or_try_init(func_ok).await });

  assert!(handle1.await.unwrap().is_err());
  assert_eq!(*handle2.await.unwrap().unwrap(), 10);
}
