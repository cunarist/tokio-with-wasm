use crate::support::{assert_pending, assert_ready, spawn};
use std::sync::Arc;
use std::task::Poll;
use tokio::sync::{RwLock, RwLockWriteGuard};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn into_inner() {
  let rwlock = RwLock::new(42);
  assert_eq!(rwlock.into_inner(), 42);
}

#[wasm_bindgen_test]
fn read_shared() {
  let rwlock = RwLock::new(100);
  let mut t1 = spawn(rwlock.read());
  let _g1 = assert_ready!(t1.poll());
  let mut t2 = spawn(rwlock.read());
  let _g2 = assert_ready!(t2.poll());
}

#[wasm_bindgen_test]
fn write_shared_pending() {
  let rwlock = RwLock::new(100);
  let mut t1 = spawn(rwlock.read());
  let _g1 = assert_ready!(t1.poll());
  let mut t2 = spawn(rwlock.write());
  assert_pending!(t2.poll());
}

#[wasm_bindgen_test]
fn read_exclusive_pending() {
  let rwlock = RwLock::new(100);
  let mut t1 = spawn(rwlock.write());
  let _g1 = assert_ready!(t1.poll());
  let mut t2 = spawn(rwlock.read());
  assert_pending!(t2.poll());
}

#[wasm_bindgen_test]
fn exhaust_reading() {
  let rwlock = RwLock::with_max_readers(100, 1024);
  let mut reads = Vec::new();
  loop {
    let mut t = spawn(rwlock.read());
    match t.poll() {
      Poll::Ready(guard) => reads.push(guard),
      Poll::Pending => break,
    }
  }

  let mut t1 = spawn(rwlock.read());
  assert_pending!(t1.poll());
  drop(reads.pop().unwrap());
  assert!(t1.is_woken());
  let _g1 = assert_ready!(t1.poll());
}

#[wasm_bindgen_test]
#[should_panic(expected = "a RwLock may not be created with 0 readers")]
fn zero_max_readers() {
  RwLock::with_max_readers(100, 0);
}

#[wasm_bindgen_test]
#[should_panic(expected = "a RwLock may not be created with 0 readers")]
fn zero_max_readers_const() {
  RwLock::const_with_max_readers(100, 0);
}

#[wasm_bindgen_test]
fn write_exclusive_pending() {
  let rwlock = RwLock::new(100);
  let mut t1 = spawn(rwlock.write());
  let _g1 = assert_ready!(t1.poll());
  let mut t2 = spawn(rwlock.write());
  assert_pending!(t2.poll());
}

#[wasm_bindgen_test]
fn write_shared_drop() {
  let rwlock = RwLock::new(100);
  let mut t1 = spawn(rwlock.read());
  let g1 = assert_ready!(t1.poll());
  let mut t2 = spawn(rwlock.write());
  assert_pending!(t2.poll());
  drop(g1);
  assert!(t2.is_woken());
  let _g2 = assert_ready!(t2.poll());
}

#[wasm_bindgen_test]
fn write_read_shared_pending() {
  let rwlock = RwLock::new(100);
  let mut t1 = spawn(rwlock.read());
  let _g1 = assert_ready!(t1.poll());
  let mut t2 = spawn(rwlock.read());
  let _g2 = assert_ready!(t2.poll());

  let mut t3 = spawn(rwlock.write());
  assert_pending!(t3.poll());

  let mut t4 = spawn(rwlock.read());
  assert_pending!(t4.poll());
}

#[wasm_bindgen_test]
fn write_read_shared_drop_pending() {
  let rwlock = RwLock::new(100);
  let mut t1 = spawn(rwlock.read());
  let _g1 = assert_ready!(t1.poll());

  let mut t2 = spawn(rwlock.write());
  assert_pending!(t2.poll());

  let mut t3 = spawn(rwlock.read());
  assert_pending!(t3.poll());
  drop(t2);

  assert!(t3.is_woken());
  let _t3 = assert_ready!(t3.poll());
}

#[wasm_bindgen_test]
async fn read_uncontested() {
  let rwlock = RwLock::new(100);
  assert_eq!(*rwlock.read().await, 100);
}

#[wasm_bindgen_test]
async fn write_uncontested() {
  let rwlock = RwLock::new(100);
  let mut result = rwlock.write().await;
  *result += 50;
  assert_eq!(*result, 150);
}

#[wasm_bindgen_test]
async fn write_order() {
  let rwlock = RwLock::<Vec<u32>>::new(vec![]);
  let fut2 = async { rwlock.write().await.push(2) };
  let fut1 = async { rwlock.write().await.push(1) };
  fut1.await;
  fut2.await;
  assert_eq!(*rwlock.read().await, vec![1, 2]);
}

#[wasm_bindgen_test]
async fn try_write() {
  let lock = RwLock::new(0);
  let read_guard = lock.read().await;
  assert!(lock.try_write().is_err());
  drop(read_guard);
  assert!(lock.try_write().is_ok());
}

#[wasm_bindgen_test]
fn try_read_try_write() {
  let lock: RwLock<usize> = RwLock::new(15);

  {
    let rg1 = lock.try_read().unwrap();
    assert_eq!(*rg1, 15);
    assert!(lock.try_write().is_err());
    let rg2 = lock.try_read().unwrap();
    assert_eq!(*rg2, 15);
  }

  {
    let mut wg = lock.try_write().unwrap();
    *wg = 1515;
    assert!(lock.try_read().is_err());
  }

  assert_eq!(*lock.try_read().unwrap(), 1515);
}

#[wasm_bindgen_test]
async fn downgrade_map() {
  let lock = RwLock::new(0);
  let write_guard = lock.write().await;
  let mut read_t = spawn(lock.read());
  assert_pending!(read_t.poll());

  let read_guard1 = RwLockWriteGuard::downgrade_map(write_guard, |v| {
    assert_pending!(read_t.poll());
    v
  });

  let read_guard2 = assert_ready!(read_t.poll());
  assert_eq!(&*read_guard1 as *const _, &*read_guard2 as *const _);
}

#[wasm_bindgen_test]
async fn try_downgrade_map() {
  let lock = RwLock::new(0);
  let write_guard = lock.write().await;
  let mut read_t = spawn(lock.read());
  assert_pending!(read_t.poll());

  let write_guard = RwLockWriteGuard::try_downgrade_map(write_guard, |_| {
    assert_pending!(read_t.poll());
    None::<&()>
  })
  .expect_err("downgrade didn't fail");
  assert_pending!(read_t.poll());

  let read_guard1 =
    RwLockWriteGuard::try_downgrade_map(write_guard, |v| Some(v))
      .expect("downgrade didn't succeed");
  let read_guard2 = assert_ready!(read_t.poll());
  assert_eq!(&*read_guard1 as *const _, &*read_guard2 as *const _);
}

#[wasm_bindgen_test]
async fn main_thread_waits_for_worker_write() {
  let lock = Arc::new(RwLock::new(0));
  let (tx, rx) = tokio::sync::oneshot::channel();
  let worker = {
    let lock = lock.clone();
    tokio::task::spawn_blocking(move || {
      let mut guard = lock.blocking_write();
      tx.send(()).unwrap();
      std::thread::sleep(std::time::Duration::from_millis(50));
      *guard = 9;
    })
  };
  rx.await.unwrap();
  assert_eq!(*lock.read().await, 9);
  worker.await.unwrap();
}
