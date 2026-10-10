use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn no_permits() {
  Semaphore::new(0);
}

#[wasm_bindgen_test]
fn try_acquire() {
  let sem = Semaphore::new(1);
  {
    let p1 = sem.try_acquire();
    assert!(p1.is_ok());
    let p2 = sem.try_acquire();
    assert!(p2.is_err());
  }
  assert!(sem.try_acquire().is_ok());
}

#[wasm_bindgen_test]
async fn acquire() {
  let sem = Arc::new(Semaphore::new(1));
  let p1 = sem.try_acquire().unwrap();
  let sem2 = sem.clone();
  let j = tokio::spawn(async move {
    let _p2 = sem2.acquire().await;
  });
  drop(p1);
  j.await.unwrap();
}

#[wasm_bindgen_test]
async fn add_permits() {
  let sem = Arc::new(Semaphore::new(0));
  let sem2 = sem.clone();
  let j = tokio::spawn(async move {
    let _p = sem2.acquire().await;
  });
  sem.add_permits(1);
  j.await.unwrap();
}

#[wasm_bindgen_test]
fn add_permits_open() {
  for size in 0..4 {
    for add in 0..4 {
      let sem = Semaphore::new(size);
      sem.add_permits(add);
      assert_eq!(sem.available_permits(), size + add);
      assert!(!sem.is_closed());
    }
  }
}

#[wasm_bindgen_test]
fn add_permits_closed() {
  for size in 0..4 {
    for add in 0..4 {
      let sem = Semaphore::new(size);
      sem.close();
      sem.add_permits(add);
      assert_eq!(sem.available_permits(), size + add);
      assert!(sem.is_closed());
    }
  }
}

#[wasm_bindgen_test]
fn forget() {
  let sem = Semaphore::new(1);
  {
    let p = sem.try_acquire().unwrap();
    assert_eq!(sem.available_permits(), 0);
    p.forget();
    assert_eq!(sem.available_permits(), 0);
  }
  assert_eq!(sem.available_permits(), 0);
  assert!(sem.try_acquire().is_err());
}

#[wasm_bindgen_test]
fn forget_permits() {
  for closed in [false, true] {
    for size in 0..4 {
      for sub in 0..4 {
        let sem = Semaphore::new(size);
        if closed {
          sem.close();
        }
        let actual = sem.forget_permits(sub);
        let expected = size.saturating_sub(sub);
        assert_eq!(sem.available_permits(), expected, "case: {size}-{sub}");
        assert_eq!(actual, size - expected);
        assert_eq!(sem.is_closed(), closed);
      }
    }
  }
}

#[wasm_bindgen_test]
fn merge() {
  let sem = Semaphore::new(3);
  {
    let mut p1 = sem.try_acquire().unwrap();
    assert_eq!(sem.available_permits(), 2);
    let p2 = sem.try_acquire_many(2).unwrap();
    assert_eq!(sem.available_permits(), 0);
    p1.merge(p2);
    assert_eq!(sem.available_permits(), 0);
  }
  assert_eq!(sem.available_permits(), 3);
}

#[wasm_bindgen_test]
fn split() {
  let sem = Semaphore::new(5);
  let mut p1 = sem.try_acquire_many(3).unwrap();
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
        let _p = sem.acquire().await;
      })
    })
    .collect();
  for j in handles {
    j.await.unwrap();
  }
  let _permits: Vec<_> = (0..5).map(|_| sem.try_acquire().unwrap()).collect();
  assert!(sem.try_acquire().is_err());
}

#[wasm_bindgen_test]
fn add_max_amount_permits() {
  let s = Semaphore::new(0);
  s.add_permits(Semaphore::MAX_PERMITS);
  assert_eq!(s.available_permits(), Semaphore::MAX_PERMITS);
}

#[wasm_bindgen_test]
fn no_panic_at_maxpermits() {
  let _ = Semaphore::new(Semaphore::MAX_PERMITS);
  let s = Semaphore::new(Semaphore::MAX_PERMITS - 1);
  s.add_permits(1);
}

#[wasm_bindgen_test]
async fn worker_releases_permit_to_main_thread() {
  let sem = Arc::new(Semaphore::new(1));
  let permit = sem.clone().try_acquire_owned().unwrap();
  tokio::task::spawn_blocking(move || drop(permit));
  let _permit = sem.acquire().await.unwrap();
}

#[wasm_bindgen_test]
async fn close_wakes_waiter() {
  let sem = Arc::new(Semaphore::new(0));
  let sem2 = sem.clone();
  let j = tokio::spawn(async move { sem2.acquire().await.is_err() });
  tokio::task::spawn_blocking(move || sem.close());
  assert!(j.await.unwrap());
}

#[wasm_bindgen_test]
fn merge_many_permits() {
  let sem = Semaphore::new(Semaphore::MAX_PERMITS);
  let half = Semaphore::MAX_PERMITS / 2;
  let mut a = sem.try_acquire_many(half as u32).unwrap();
  let b = sem.try_acquire_many(half as u32).unwrap();
  a.merge(b);
  assert_eq!(a.num_permits(), half * 2);
  drop(a);
  assert_eq!(sem.available_permits(), Semaphore::MAX_PERMITS);
}
