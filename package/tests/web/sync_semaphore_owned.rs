use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn try_acquire() {
  let sem = Arc::new(Semaphore::new(1));
  {
    let p1 = sem.clone().try_acquire_owned();
    assert!(p1.is_ok());
    let p2 = sem.clone().try_acquire_owned();
    assert!(p2.is_err());
  }
  assert!(sem.try_acquire_owned().is_ok());
}

#[wasm_bindgen_test]
fn try_acquire_many() {
  let sem = Arc::new(Semaphore::new(42));
  {
    let p1 = sem.clone().try_acquire_many_owned(42);
    assert!(p1.is_ok());
    let p2 = sem.clone().try_acquire_owned();
    assert!(p2.is_err());
  }
  let p3 = sem.clone().try_acquire_many_owned(32);
  assert!(p3.is_ok());
  let p4 = sem.clone().try_acquire_many_owned(10);
  assert!(p4.is_ok());
  assert!(sem.try_acquire_owned().is_err());
}

#[wasm_bindgen_test]
async fn acquire() {
  let sem = Arc::new(Semaphore::new(1));
  let p1 = sem.clone().try_acquire_owned().unwrap();
  let j = tokio::spawn(async move {
    let _p2 = sem.acquire_owned().await;
  });
  drop(p1);
  j.await.unwrap();
}

#[wasm_bindgen_test]
async fn acquire_many() {
  let sem = Arc::new(Semaphore::new(42));
  let permit32 = sem.clone().try_acquire_many_owned(32).unwrap();
  let (tx, rx) = tokio::sync::oneshot::channel();
  let j = tokio::spawn(async move {
    let _permit10 = sem.clone().acquire_many_owned(10).await.unwrap();
    tx.send(()).unwrap();
    let _permit32 = sem.acquire_many_owned(32).await.unwrap();
  });
  rx.await.unwrap();
  drop(permit32);
  j.await.unwrap();
}

#[wasm_bindgen_test]
async fn add_permits() {
  let sem = Arc::new(Semaphore::new(0));
  let sem2 = sem.clone();
  let j = tokio::spawn(async move {
    let _p = sem2.acquire_owned().await;
  });
  sem.add_permits(1);
  j.await.unwrap();
}

#[wasm_bindgen_test]
fn forget() {
  let sem = Arc::new(Semaphore::new(1));
  {
    let p = sem.clone().try_acquire_owned().unwrap();
    assert_eq!(sem.available_permits(), 0);
    p.forget();
    assert_eq!(sem.available_permits(), 0);
  }
  assert_eq!(sem.available_permits(), 0);
  assert!(sem.try_acquire_owned().is_err());
}

#[wasm_bindgen_test]
fn merge() {
  let sem = Arc::new(Semaphore::new(3));
  {
    let mut p1 = sem.clone().try_acquire_owned().unwrap();
    assert_eq!(sem.available_permits(), 2);
    let p2 = sem.clone().try_acquire_many_owned(2).unwrap();
    assert_eq!(sem.available_permits(), 0);
    p1.merge(p2);
    assert_eq!(sem.available_permits(), 0);
  }
  assert_eq!(sem.available_permits(), 3);
}

#[wasm_bindgen_test]
fn split() {
  let sem = Arc::new(Semaphore::new(5));
  let mut p1 = sem.clone().try_acquire_many_owned(3).unwrap();
  assert_eq!(sem.available_permits(), 2);
  assert_eq!(p1.num_permits(), 3);
  let mut p2 = p1.split(1).unwrap();
  assert_eq!(sem.available_permits(), 2);
  assert_eq!(p1.num_permits(), 2);
  assert_eq!(p2.num_permits(), 1);
  let p3 = p1.split(0).unwrap();
  assert_eq!(p3.num_permits(), 0);
  drop(p1);
  assert_eq!(sem.available_permits(), 4);
  let p4 = p2.split(1).unwrap();
  assert_eq!(p2.num_permits(), 0);
  assert_eq!(p4.num_permits(), 1);
  assert!(p2.split(1).is_none());
  drop(p2);
  assert_eq!(sem.available_permits(), 4);
  drop(p3);
  assert_eq!(sem.available_permits(), 4);
  drop(p4);
  assert_eq!(sem.available_permits(), 5);
}

#[wasm_bindgen_test]
async fn stress_test() {
  let sem = Arc::new(Semaphore::new(5));
  let handles: Vec<_> = (0..1000)
    .map(|_| {
      let sem = sem.clone();
      tokio::spawn(async move {
        let _p = sem.acquire_owned().await;
      })
    })
    .collect();
  for j in handles {
    j.await.unwrap();
  }
  let _permits: Vec<_> = (0..5)
    .map(|_| sem.clone().try_acquire_owned().unwrap())
    .collect();
  assert!(sem.try_acquire_owned().is_err());
}

#[wasm_bindgen_test]
async fn permit_moves_to_a_web_worker() {
  let sem = Arc::new(Semaphore::new(1));
  let permit = sem.clone().try_acquire_owned().unwrap();
  tokio::task::spawn_blocking(move || drop(permit));
  let _permit = sem.acquire_owned().await.unwrap();
}
