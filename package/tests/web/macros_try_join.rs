use tokio::sync::oneshot;
use tokio::task::yield_now;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

fn ok<T>(val: T) -> Result<T, ()> {
  Ok(val)
}

#[wasm_bindgen_test]
async fn sync_one_lit_expr_comma() {
  let x = tokio::try_join!(async { ok(1) },);
  assert_eq!(x, Ok((1,)));
  let x = tokio::try_join!(biased; async { ok(1) },);
  assert_eq!(x, Ok((1,)));
}

#[wasm_bindgen_test]
async fn sync_one_lit_expr_no_comma() {
  let x = tokio::try_join!(async { ok(1) });
  assert_eq!(x, Ok((1,)));
  let x = tokio::try_join!(biased; async { ok(1) });
  assert_eq!(x, Ok((1,)));
}

#[wasm_bindgen_test]
async fn sync_two_lit_expr_comma() {
  let x = tokio::try_join!(async { ok(1) }, async { ok(2) },);
  assert_eq!(x, Ok((1, 2)));
  let x = tokio::try_join!(biased; async { ok(1) }, async { ok(2) },);
  assert_eq!(x, Ok((1, 2)));
}

#[wasm_bindgen_test]
async fn sync_two_lit_expr_no_comma() {
  let x = tokio::try_join!(async { ok(1) }, async { ok(2) });
  assert_eq!(x, Ok((1, 2)));
  let x = tokio::try_join!(biased; async { ok(1) }, async { ok(2) });
  assert_eq!(x, Ok((1, 2)));
}

#[wasm_bindgen_test]
async fn two_await() {
  let (tx1, rx1) = oneshot::channel::<&str>();
  let (tx2, rx2) = oneshot::channel::<u32>();
  tokio::spawn(async move {
    tx2.send(123).unwrap();
    yield_now().await;
    tx1.send("hello").unwrap();
  });
  assert_eq!(tokio::try_join!(rx1, rx2), Ok(("hello", 123)));
}

#[wasm_bindgen_test]
async fn err_abort_early() {
  let (tx1, rx1) = oneshot::channel::<&str>();
  let (tx2, rx2) = oneshot::channel::<u32>();
  let (_tx3, rx3) = oneshot::channel::<u32>();
  tokio::spawn(async move {
    tx2.send(123).unwrap();
    yield_now().await;
    drop(tx1);
  });
  assert!(tokio::try_join!(rx1, rx2, rx3).is_err());
}

#[wasm_bindgen_test]
async fn blocking_task_error_aborts_early() {
  let res =
    tokio::try_join!(std::future::pending::<Result<(), &str>>(), async {
      tokio::task::spawn_blocking(|| Err::<(), _>("failed"))
        .await
        .unwrap()
    },);
  assert_eq!(res, Err("failed"));
}

#[wasm_bindgen_test]
async fn empty_try_join() {
  assert_eq!(tokio::try_join!() as Result<_, ()>, Ok(()));
  assert_eq!(tokio::try_join!(biased;) as Result<_, ()>, Ok(()));
}

#[wasm_bindgen_test]
async fn caller_names_const_count() {
  let (tx, rx) = oneshot::channel::<u32>();
  const COUNT: u32 = 2;
  tokio::try_join!(async { tx.send(COUNT) }).unwrap();
  assert_eq!(2, rx.await.unwrap());
}
