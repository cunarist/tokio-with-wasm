//! Browser tests for `io`.
//! Run with `wasm-pack test --headless --chrome package`.
//!
//! These check that the re-export is wired up, not that `tokio` works.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::io::{
  AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, copy, duplex,
};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn reads_a_slice_to_the_end() {
  let mut source: &[u8] = b"hello";
  let mut read = Vec::new();
  let count = match source.read_to_end(&mut read).await {
    Ok(count) => count,
    Err(error) => panic!("could not read: {error}"),
  };
  assert_eq!(count, 5);
  assert_eq!(read, b"hello");
}

#[wasm_bindgen_test]
async fn reads_lines_through_a_buffer() {
  let mut lines = BufReader::new(b"first\nsecond\n".as_slice()).lines();
  let mut read = Vec::new();
  while let Ok(Some(line)) = lines.next_line().await {
    read.push(line);
  }
  assert_eq!(read, ["first", "second"]);
}

#[wasm_bindgen_test]
async fn copies_between_the_halves_of_a_duplex() {
  let (mut client, mut server) = duplex(64);
  if let Err(error) = client.write_all(b"ping").await {
    panic!("could not write: {error}");
  }
  if let Err(error) = client.shutdown().await {
    panic!("could not shut down: {error}");
  }

  let mut copied = Vec::new();
  let count = match copy(&mut server, &mut copied).await {
    Ok(count) => count,
    Err(error) => panic!("could not copy: {error}"),
  };
  assert_eq!(count, 4);
  assert_eq!(copied, b"ping");
}
