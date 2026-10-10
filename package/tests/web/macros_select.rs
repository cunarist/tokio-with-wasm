use std::cell::Cell;
use std::future::{pending, poll_fn};
use std::task::Poll::Ready;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::yield_now;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

async fn one() -> usize {
  1
}

async fn require_mutable(_: &mut i32) {}
async fn async_noop() {}

async fn async_never() -> ! {
  pending().await
}

#[wasm_bindgen_test]
async fn sync_one_lit_expr_comma() {
  let x = tokio::select! {
    x = async { 1 } => x,
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn no_branch_else_only() {
  let x = tokio::select! {
    else => 1,
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn no_branch_else_only_biased() {
  let x = tokio::select! {
    biased;
    else => 1,
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn nested_one() {
  let x = tokio::select! {
    x = async { 1 } => tokio::select! {
      y = async { x } => y,
    },
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn sync_one_lit_expr_no_comma() {
  let x = tokio::select! {
    x = async { 1 } => x
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn sync_one_lit_expr_block() {
  let x = tokio::select! {
    x = async { 1 } => { x }
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn sync_one_await() {
  let x = tokio::select! {
    x = one() => x,
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn sync_one_ident() {
  let one = one();
  let x = tokio::select! {
    x = one => x,
  };
  assert_eq!(x, 1);
}

#[wasm_bindgen_test]
async fn sync_two() {
  let cnt = Cell::new(0);
  let res = tokio::select! {
    x = async {
      cnt.set(cnt.get() + 1);
      1
    } => x,
    y = async {
      cnt.set(cnt.get() + 1);
      2
    } => y,
  };
  assert_eq!(1, cnt.get());
  assert!(res == 1 || res == 2);
}

#[wasm_bindgen_test]
async fn drop_in_fut() {
  let s = "hello".to_string();
  let res = tokio::select! {
    x = async {
      let v = one().await;
      drop(s);
      v
    } => x
  };
  assert_eq!(res, 1);
}

#[wasm_bindgen_test]
async fn one_ready() {
  let (tx1, rx1) = oneshot::channel::<i32>();
  let (_tx2, rx2) = oneshot::channel::<i32>();
  tx1.send(1).unwrap();
  let v = tokio::select! {
    res = rx1 => res.unwrap(),
    _ = rx2 => unreachable!(),
  };
  assert_eq!(1, v);
}

#[wasm_bindgen_test]
async fn select_streams() {
  let (tx1, mut rx1) = mpsc::unbounded_channel::<i32>();
  let (tx2, mut rx2) = mpsc::unbounded_channel::<i32>();
  tokio::spawn(async move {
    tx2.send(1).unwrap();
    yield_now().await;
    tx1.send(2).unwrap();
    yield_now().await;
    tx2.send(3).unwrap();
    yield_now().await;
    drop((tx1, tx2));
  });

  let mut rem = true;
  let mut msgs = vec![];
  while rem {
    tokio::select! {
      Some(x) = rx1.recv() => msgs.push(x),
      Some(y) = rx2.recv() => msgs.push(y),
      else => rem = false,
    }
  }
  msgs.sort_unstable();
  assert_eq!(&msgs[..], &[1, 2, 3]);
}

#[wasm_bindgen_test]
async fn move_uncompleted_futures() {
  let (tx1, mut rx1) = oneshot::channel::<i32>();
  let (tx2, mut rx2) = oneshot::channel::<i32>();
  tx1.send(1).unwrap();
  tx2.send(2).unwrap();

  let ran;
  tokio::select! {
    res = &mut rx1 => {
      assert_eq!(1, res.unwrap());
      assert_eq!(2, rx2.await.unwrap());
      ran = true;
    },
    res = &mut rx2 => {
      assert_eq!(2, res.unwrap());
      assert_eq!(1, rx1.await.unwrap());
      ran = true;
    },
  }
  assert!(ran);
}

#[wasm_bindgen_test]
async fn nested() {
  let res = tokio::select! {
    x = async { 1 } => {
      tokio::select! {
        y = async { 2 } => x + y,
      }
    }
  };
  assert_eq!(res, 3);
}

#[wasm_bindgen_test]
async fn mutable_borrowing_future_with_same_borrow_in_block() {
  let mut value = 234;
  tokio::select! {
    _ = require_mutable(&mut value) => { },
    _ = async_noop() => {
      value += 5;
    },
  }
  assert!(value >= 234);
}

#[wasm_bindgen_test]
async fn mutable_borrowing_future_with_same_borrow_in_block_and_else() {
  let mut value = 234;
  tokio::select! {
    _ = require_mutable(&mut value) => { },
    _ = async_noop() => {
      value += 5;
    },
    else => {
      value += 27;
    },
  }
  assert!(value >= 234);
}

#[wasm_bindgen_test]
async fn future_panics_after_poll() {
  let (tx, rx) = oneshot::channel();
  let mut polled = false;
  let f = poll_fn(|_| {
    assert!(!polled);
    polled = true;
    Ready(None::<()>)
  });
  tokio::spawn(async move {
    yield_now().await;
    tx.send(1).unwrap();
  });
  let res = tokio::select! {
    Some(_) = f => unreachable!(),
    ret = rx => ret.unwrap(),
  };
  assert_eq!(1, res);
}

#[wasm_bindgen_test]
async fn disable_with_if() {
  let f = poll_fn(|_| panic!());
  let (tx, rx) = oneshot::channel();
  tokio::spawn(async move {
    yield_now().await;
    tx.send(()).unwrap();
  });
  tokio::select! {
    _ = f, if false => unreachable!(),
    _ = rx => (),
  }
}

#[wasm_bindgen_test]
async fn join_with_select() {
  let (tx1, mut rx1) = oneshot::channel();
  let (tx2, mut rx2) = oneshot::channel();
  tokio::spawn(async move {
    tx1.send(123).unwrap();
    yield_now().await;
    tx2.send(456).unwrap();
  });

  let mut a = None;
  let mut b = None;
  while a.is_none() || b.is_none() {
    tokio::select! {
      v1 = &mut rx1, if a.is_none() => a = Some(v1.unwrap()),
      v2 = &mut rx2, if b.is_none() => b = Some(v2.unwrap()),
    }
  }
  assert_eq!(a, Some(123));
  assert_eq!(b, Some(456));
}

#[wasm_bindgen_test]
async fn use_future_in_if_condition() {
  tokio::select! {
    _ = tokio::time::sleep(Duration::from_millis(10)), if false => {
      panic!("if condition ignored")
    }
    _ = async { 1u32 } => {}
  }
}

#[wasm_bindgen_test]
async fn use_future_in_if_condition_biased() {
  tokio::select! {
    biased;
    _ = tokio::time::sleep(Duration::from_millis(10)), if false => {
      panic!("if condition ignored")
    }
    _ = async { 1u32 } => {}
  }
}

#[wasm_bindgen_test]
async fn many_branches() {
  let num = tokio::select! {
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
    x = async { 1 } => x,
  };
  assert_eq!(1, num);
}

#[wasm_bindgen_test]
async fn never_branch_no_warnings() {
  let t = tokio::select! {
    _ = async_never() => 0,
    one_async_ready = one() => one_async_ready,
  };
  assert_eq!(t, 1);
}

#[wasm_bindgen_test]
async fn mut_on_left_hand_side() {
  let ok = async { 1 };
  tokio::pin!(ok);
  let v = tokio::select! {
    mut a = &mut ok => {
      a += 1;
      a
    }
  };
  assert_eq!(v, 2);
}

#[wasm_bindgen_test]
async fn biased_one_not_ready() {
  let (_tx1, rx1) = oneshot::channel::<i32>();
  let (tx2, rx2) = oneshot::channel::<i32>();
  let (tx3, rx3) = oneshot::channel::<i32>();
  tx2.send(2).unwrap();
  tx3.send(3).unwrap();
  let v = tokio::select! {
    biased;
    _ = rx1 => unreachable!(),
    res = rx2 => res.unwrap(),
    _ = rx3 => panic!("`biased;` should poll `rx2` before `rx3`"),
  };
  assert_eq!(2, v);
}

#[wasm_bindgen_test]
async fn biased_eventually_ready() {
  let one = async {};
  let two = async { yield_now().await };
  let three = async { yield_now().await };
  let mut count = 0u8;
  tokio::pin!(one, two, three);
  loop {
    tokio::select! {
      biased;
      _ = &mut two, if count < 2 => {
        count += 1;
        assert_eq!(count, 2);
      }
      _ = &mut three, if count < 3 => {
        count += 1;
        assert_eq!(count, 3);
      }
      _ = &mut one, if count < 1 => {
        count += 1;
        assert_eq!(count, 1);
      }
      else => break,
    }
  }
  assert_eq!(count, 3);
}

#[wasm_bindgen_test]
async fn mut_ref_patterns() {
  tokio::select! {
    Some(mut x) = async { Some("1".to_string()) } => {
      assert_eq!(x, "1");
      x = "2".to_string();
      assert_eq!(x, "2");
    },
  };
  tokio::select! {
    Some(ref x) = async { Some("1".to_string()) } => {
      assert_eq!(*x, "1");
    },
  };
  tokio::select! {
    Some(ref mut x) = async { Some("1".to_string()) } => {
      assert_eq!(*x, "1");
      *x = "2".to_string();
      assert_eq!(*x, "2");
    },
  };
}

#[wasm_bindgen_test]
async fn select_into_future() {
  struct NotAFuture;
  impl std::future::IntoFuture for NotAFuture {
    type Output = ();
    type IntoFuture = std::future::Ready<()>;

    fn into_future(self) -> Self::IntoFuture {
      std::future::ready(())
    }
  }

  tokio::select! {
    () = NotAFuture => {},
  }
}

#[wasm_bindgen_test]
async fn temporary_lifetime_extension() {
  tokio::select! {
    () = &mut std::future::ready(()) => {},
  }
}

#[wasm_bindgen_test]
async fn sleep_wins_over_pending() {
  tokio::select! {
    _ = pending::<()>() => unreachable!(),
    _ = tokio::time::sleep(Duration::from_millis(1)) => {}
  }
}

#[wasm_bindgen_test]
async fn blocking_task_wins_over_pending() {
  let v = tokio::select! {
    _ = pending::<()>() => unreachable!(),
    res = tokio::task::spawn_blocking(|| 1) => res.unwrap(),
  };
  assert_eq!(v, 1);
}
