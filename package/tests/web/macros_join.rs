use std::time::Duration;
use tokio::sync::oneshot;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn sync_one_lit_expr_comma() {
  let x = tokio::join!(async { 1 },);
  assert_eq!(x, (1,));
  let x = tokio::join!(biased; async { 1 },);
  assert_eq!(x, (1,));
}

#[wasm_bindgen_test]
async fn sync_one_lit_expr_no_comma() {
  let x = tokio::join!(async { 1 });
  assert_eq!(x, (1,));
  let x = tokio::join!(biased; async { 1 });
  assert_eq!(x, (1,));
}

#[wasm_bindgen_test]
async fn sync_two_lit_expr_comma() {
  let x = tokio::join!(async { 1 }, async { 2 },);
  assert_eq!(x, (1, 2));
  let x = tokio::join!(biased; async { 1 }, async { 2 },);
  assert_eq!(x, (1, 2));
}

#[wasm_bindgen_test]
async fn sync_two_lit_expr_no_comma() {
  let x = tokio::join!(async { 1 }, async { 2 });
  assert_eq!(x, (1, 2));
  let x = tokio::join!(biased; async { 1 }, async { 2 });
  assert_eq!(x, (1, 2));
}

#[wasm_bindgen_test]
async fn two_await() {
  let (tx1, rx1) = oneshot::channel::<&str>();
  let (tx2, rx2) = oneshot::channel::<u32>();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(1)).await;
    tx2.send(123).unwrap();
    tx1.send("hello").unwrap();
  });
  let res =
    tokio::join!(async { rx1.await.unwrap() }, async { rx2.await.unwrap() });
  assert_eq!(res, ("hello", 123));
}

#[wasm_bindgen_test]
async fn join_blocking_tasks() {
  let res = tokio::join!(
    tokio::task::spawn_blocking(|| 1),
    tokio::task::spawn_blocking(|| 2),
  );
  assert_eq!((res.0.unwrap(), res.1.unwrap()), (1, 2));
}

#[wasm_bindgen_test]
#[allow(clippy::unit_cmp)]
async fn empty_join() {
  assert_eq!(tokio::join!(), ());
  assert_eq!(tokio::join!(biased;), ());
}

#[wasm_bindgen_test]
async fn join_into_future() {
  struct NotAFuture;
  impl std::future::IntoFuture for NotAFuture {
    type Output = ();
    type IntoFuture = std::future::Ready<()>;

    fn into_future(self) -> Self::IntoFuture {
      std::future::ready(())
    }
  }

  tokio::join!(NotAFuture);
}

#[wasm_bindgen_test]
async fn caller_names_const_count() {
  let (tx, rx) = oneshot::channel::<u32>();
  const COUNT: u32 = 2;
  tokio::join!(async { tx.send(COUNT).unwrap() });
  assert_eq!(2, rx.await.unwrap());
}
