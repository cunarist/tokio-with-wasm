//! Browser tests for `fs`.
//! Run with `wasm-pack test --headless --chrome package`.
//!
//! Every test works inside a directory of its own, because the origin
//! private file system outlives the test that wrote to it.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::io::{ErrorKind, SeekFrom};
use std::path::PathBuf;
use tokio_with_wasm::fs;
use tokio_with_wasm::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_with_wasm::time::{Duration, timeout};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// Hands back an empty directory named after the test that asked for it.
async fn scratch(name: &str) -> PathBuf {
  let path = PathBuf::from(name);
  if fs::remove_dir_all(&path).await.is_ok() {
    // The leftovers of an earlier run are gone.
  }
  if let Err(failure) = fs::create_dir_all(&path).await {
    panic!("could not prepare `{name}`: {failure}");
  }
  path
}

/// Unwraps without tripping the lint against `unwrap` in this repository.
fn ok<T>(result: std::io::Result<T>, what: &str) -> T {
  match result {
    Ok(value) => value,
    Err(failure) => panic!("could not {what}: {failure}"),
  }
}

/// Reads the kind off a call that was supposed to fail.
fn kind<T>(result: std::io::Result<T>) -> ErrorKind {
  match result {
    Ok(_) => panic!("the call was supposed to fail"),
    Err(failure) => failure.kind(),
  }
}

#[wasm_bindgen_test]
async fn writes_and_reads_a_file_back() {
  let directory = scratch("writes_and_reads").await;
  let path = directory.join("greeting.txt");

  ok(fs::write(&path, b"hello").await, "write");
  assert_eq!(ok(fs::read(&path).await, "read"), b"hello");
  assert_eq!(ok(fs::read_to_string(&path).await, "read text"), "hello");
}

#[wasm_bindgen_test]
async fn writing_again_replaces_everything() {
  let directory = scratch("writing_again").await;
  let path = directory.join("notes.txt");

  ok(fs::write(&path, b"the long first draft").await, "write");
  ok(fs::write(&path, b"short").await, "write again");
  assert_eq!(ok(fs::read_to_string(&path).await, "read"), "short");
}

#[wasm_bindgen_test]
async fn a_leading_slash_names_the_same_file() {
  let directory = scratch("leading_slash").await;
  ok(
    fs::write(directory.join("same.txt"), b"once").await,
    "write",
  );

  let absolute = PathBuf::from("/").join(&directory).join("same.txt");
  assert_eq!(ok(fs::read_to_string(absolute).await, "read"), "once");
}

#[wasm_bindgen_test]
async fn a_path_cannot_lead_out_of_the_root() {
  let failure = fs::read("../secret").await;
  match failure {
    Err(failure) => assert_eq!(failure.kind(), ErrorKind::InvalidInput),
    Ok(_) => panic!("reading outside the root should not work"),
  }
}

#[wasm_bindgen_test]
async fn reading_a_missing_file_reports_not_found() {
  let directory = scratch("missing_file").await;
  match fs::read(directory.join("nothing.txt")).await {
    Err(failure) => assert_eq!(failure.kind(), ErrorKind::NotFound),
    Ok(_) => panic!("there is nothing to read"),
  }
}

#[wasm_bindgen_test]
async fn reports_what_it_knows_about_an_entry() {
  let directory = scratch("metadata").await;
  let path = directory.join("sized.bin");
  ok(fs::write(&path, b"12345").await, "write");

  let about_file = ok(fs::metadata(&path).await, "read metadata");
  assert!(about_file.is_file());
  assert!(!about_file.is_dir());
  assert_eq!(about_file.len(), 5);
  assert!(about_file.modified().is_ok());

  let about_directory = ok(fs::metadata(&directory).await, "read metadata");
  assert!(about_directory.is_dir());
  assert!(!about_directory.is_file());
  assert!(about_directory.modified().is_err());
}

#[wasm_bindgen_test]
async fn tells_whether_an_entry_is_there() {
  let directory = scratch("try_exists").await;
  let path = directory.join("here.txt");
  assert!(!ok(fs::try_exists(&path).await, "look"));
  ok(fs::write(&path, b"x").await, "write");
  assert!(ok(fs::try_exists(&path).await, "look again"));
}

#[wasm_bindgen_test]
async fn lists_what_a_directory_holds() {
  let directory = scratch("read_dir").await;
  ok(fs::write(directory.join("a.txt"), b"a").await, "write");
  ok(fs::write(directory.join("b.txt"), b"b").await, "write");
  ok(fs::create_dir(directory.join("inner")).await, "create");

  let mut found = Vec::new();
  let mut entries = ok(fs::read_dir(&directory).await, "read the directory");
  while let Some(entry) = ok(entries.next_entry().await, "step") {
    let kind = ok(entry.file_type().await, "read the kind");
    found.push((
      entry.file_name().to_string_lossy().into_owned(),
      kind.is_dir(),
    ));
  }
  found.sort();

  assert_eq!(
    found,
    [
      ("a.txt".to_owned(), false),
      ("b.txt".to_owned(), false),
      ("inner".to_owned(), true),
    ]
  );
}

#[wasm_bindgen_test]
async fn an_entry_knows_the_path_that_leads_to_it() {
  let directory = scratch("entry_path").await;
  ok(fs::write(directory.join("only.txt"), b"x").await, "write");

  let mut entries = ok(fs::read_dir(&directory).await, "read the directory");
  let entry = match ok(entries.next_entry().await, "step") {
    Some(entry) => entry,
    None => panic!("the directory should hold one entry"),
  };
  assert_eq!(entry.path(), PathBuf::from("/entry_path/only.txt"));
}

#[wasm_bindgen_test]
async fn creating_a_directory_twice_reports_it_is_there() {
  let directory = scratch("create_dir_twice").await;
  let inner = directory.join("once");
  ok(fs::create_dir(&inner).await, "create");
  match fs::create_dir(&inner).await {
    Err(failure) => assert_eq!(failure.kind(), ErrorKind::AlreadyExists),
    Ok(()) => panic!("the directory is already there"),
  }
}

#[wasm_bindgen_test]
async fn removes_files_and_directories() {
  let directory = scratch("removing").await;
  let file = directory.join("gone.txt");
  ok(fs::write(&file, b"x").await, "write");
  ok(fs::remove_file(&file).await, "remove the file");
  assert!(!ok(fs::try_exists(&file).await, "look"));

  let empty = directory.join("empty");
  ok(fs::create_dir(&empty).await, "create");
  ok(fs::remove_dir(&empty).await, "remove the directory");
  assert!(!ok(fs::try_exists(&empty).await, "look"));
}

#[wasm_bindgen_test]
async fn a_directory_that_holds_something_needs_removing_in_full() {
  let directory = scratch("remove_full").await;
  let nested = directory.join("nested");
  ok(fs::create_dir(&nested).await, "create");
  ok(fs::write(nested.join("kept.txt"), b"x").await, "write");

  assert!(fs::remove_dir(&nested).await.is_err());
  ok(fs::remove_dir_all(&nested).await, "remove everything");
  assert!(!ok(fs::try_exists(&nested).await, "look"));
}

#[wasm_bindgen_test]
async fn removing_a_file_as_a_directory_does_not_work() {
  let directory = scratch("remove_wrong_kind").await;
  let file = directory.join("file.txt");
  ok(fs::write(&file, b"x").await, "write");

  assert!(fs::remove_dir(&file).await.is_err());
  assert!(fs::remove_file(&directory).await.is_err());
  assert!(ok(fs::try_exists(&file).await, "look"));
}

#[wasm_bindgen_test]
async fn copies_a_file() {
  let directory = scratch("copying").await;
  let from = directory.join("from.txt");
  let to = directory.join("to.txt");
  ok(fs::write(&from, b"carried over").await, "write");

  let length = ok(fs::copy(&from, &to).await, "copy");
  assert_eq!(length, 12);
  assert_eq!(ok(fs::read_to_string(&to).await, "read"), "carried over");
  assert!(ok(fs::try_exists(&from).await, "look"));
}

#[wasm_bindgen_test]
async fn renames_a_file() {
  let directory = scratch("renaming_file").await;
  let from = directory.join("before.txt");
  let to = directory.join("after.txt");
  ok(fs::write(&from, b"moved").await, "write");

  ok(fs::rename(&from, &to).await, "rename");
  assert_eq!(ok(fs::read_to_string(&to).await, "read"), "moved");
  assert!(!ok(fs::try_exists(&from).await, "look"));
}

#[wasm_bindgen_test]
async fn renames_a_directory_and_everything_under_it() {
  let directory = scratch("renaming_directory").await;
  let from = directory.join("before");
  let to = directory.join("after");
  ok(fs::create_dir_all(from.join("deep")).await, "create");
  ok(fs::write(from.join("top.txt"), b"top").await, "write");
  ok(fs::write(from.join("deep/low.txt"), b"low").await, "write");

  ok(fs::rename(&from, &to).await, "rename");
  assert_eq!(
    ok(fs::read_to_string(to.join("top.txt")).await, "read"),
    "top"
  );
  assert_eq!(
    ok(fs::read_to_string(to.join("deep/low.txt")).await, "read"),
    "low"
  );
  assert!(!ok(fs::try_exists(&from).await, "look"));
}

#[wasm_bindgen_test]
async fn spells_a_path_one_way() {
  let directory = scratch("canonicalize").await;
  ok(fs::write(directory.join("here.txt"), b"x").await, "write");

  let spelled = ok(
    fs::canonicalize("canonicalize/./inner/../here.txt").await,
    "spell the path",
  );
  assert_eq!(spelled, PathBuf::from("/canonicalize/here.txt"));
}

#[wasm_bindgen_test]
async fn writes_through_an_open_file() {
  let directory = scratch("open_write").await;
  let path = directory.join("streamed.txt");

  let mut file = ok(fs::File::create(&path).await, "create");
  ok(file.write_all(b"first ").await, "write");
  ok(file.write_all(b"second").await, "write again");
  ok(file.flush().await, "flush");

  assert_eq!(ok(fs::read_to_string(&path).await, "read"), "first second");
}

#[wasm_bindgen_test]
async fn dropping_a_file_still_lands_the_writes() {
  let directory = scratch("dropped_write").await;
  let path = directory.join("dropped.txt");

  let mut file = ok(fs::File::create(&path).await, "create");
  ok(file.write_all(b"left behind").await, "write");
  drop(file);

  // The commit was handed to the event loop, so let it run.
  for _ in 0..100 {
    if !ok(fs::metadata(&path).await, "read metadata").is_empty() {
      break;
    }
    tokio_with_wasm::task::yield_now().await;
  }
  assert_eq!(ok(fs::read_to_string(&path).await, "read"), "left behind");
}

#[wasm_bindgen_test]
async fn reads_through_an_open_file() {
  let directory = scratch("open_read").await;
  let path = directory.join("read.txt");
  ok(fs::write(&path, b"streamed back").await, "write");

  let mut file = ok(fs::File::open(&path).await, "open");
  let mut read = String::new();
  ok(file.read_to_string(&mut read).await, "read");
  assert_eq!(read, "streamed back");
}

#[wasm_bindgen_test]
async fn a_read_sees_what_was_just_written() {
  let directory = scratch("write_then_read").await;
  let path = directory.join("both.txt");

  let mut file = ok(
    fs::File::options()
      .read(true)
      .write(true)
      .create(true)
      .truncate(true)
      .open(&path)
      .await,
    "open",
  );
  ok(file.write_all(b"abcdef").await, "write");
  ok(file.seek(SeekFrom::Start(2)).await, "seek");

  let mut read = Vec::new();
  ok(file.read_to_end(&mut read).await, "read");
  assert_eq!(read, b"cdef");
}

#[wasm_bindgen_test]
async fn seeks_from_every_side() {
  let directory = scratch("seeking").await;
  let path = directory.join("seek.txt");
  ok(fs::write(&path, b"0123456789").await, "write");

  let mut file = ok(fs::File::open(&path).await, "open");
  assert_eq!(ok(file.seek(SeekFrom::Start(4)).await, "seek"), 4);
  assert_eq!(ok(file.seek(SeekFrom::Current(2)).await, "seek"), 6);
  assert_eq!(ok(file.seek(SeekFrom::End(-3)).await, "seek"), 7);

  let mut read = Vec::new();
  ok(file.read_to_end(&mut read).await, "read");
  assert_eq!(read, b"789");
}

#[wasm_bindgen_test]
async fn seeking_before_the_start_does_not_work() {
  let directory = scratch("seek_before_start").await;
  let path = directory.join("short.txt");
  ok(fs::write(&path, b"ab").await, "write");

  let mut file = ok(fs::File::open(&path).await, "open");
  assert!(file.seek(SeekFrom::Current(-1)).await.is_err());
}

#[wasm_bindgen_test]
async fn cuts_a_file_down_to_size() {
  let directory = scratch("set_len").await;
  let path = directory.join("trimmed.txt");
  ok(fs::write(&path, b"0123456789").await, "write");

  let mut file = ok(
    fs::File::options().write(true).open(&path).await,
    "open for writing",
  );
  ok(file.set_len(4).await, "cut it down");
  assert_eq!(ok(fs::read_to_string(&path).await, "read"), "0123");
}

#[wasm_bindgen_test]
async fn appending_starts_at_the_end() {
  let directory = scratch("appending").await;
  let path = directory.join("log.txt");
  ok(fs::write(&path, b"first\n").await, "write");

  let mut file = ok(
    fs::File::options().append(true).open(&path).await,
    "open for appending",
  );
  ok(file.write_all(b"second\n").await, "write");
  ok(file.flush().await, "flush");

  assert_eq!(
    ok(fs::read_to_string(&path).await, "read"),
    "first\nsecond\n"
  );
}

#[wasm_bindgen_test]
async fn creating_a_new_file_over_an_old_one_does_not_work() {
  let directory = scratch("create_new").await;
  let path = directory.join("once.txt");

  ok(
    fs::File::options()
      .write(true)
      .create_new(true)
      .open(&path)
      .await,
    "create",
  );
  match fs::File::options()
    .write(true)
    .create_new(true)
    .open(&path)
    .await
  {
    Err(failure) => assert_eq!(failure.kind(), ErrorKind::AlreadyExists),
    Ok(_) => panic!("the file is already there"),
  }
}

#[wasm_bindgen_test]
async fn a_file_opened_for_reading_refuses_writes() {
  let directory = scratch("read_only").await;
  let path = directory.join("locked.txt");
  ok(fs::write(&path, b"x").await, "write");

  let mut file = ok(fs::File::open(&path).await, "open");
  match file.write_all(b"y").await {
    Err(failure) => assert_eq!(failure.kind(), ErrorKind::PermissionDenied),
    Ok(()) => panic!("a file opened for reading should refuse writes"),
  }
}

#[wasm_bindgen_test]
async fn opening_a_file_needs_a_reason() {
  let directory = scratch("no_reason").await;
  let path = directory.join("idle.txt");
  ok(fs::write(&path, b"x").await, "write");

  match fs::File::options().open(&path).await {
    Err(failure) => assert_eq!(failure.kind(), ErrorKind::InvalidInput),
    Ok(_) => panic!("a file has to be opened for something"),
  }
}

#[wasm_bindgen_test]
async fn writes_more_than_the_buffer_holds() {
  let directory = scratch("large_write").await;
  let path = directory.join("large.bin");
  let block = vec![7u8; 300 * 1024];

  let mut file = ok(fs::File::create(&path).await, "create");
  for _ in 0..5 {
    ok(file.write_all(&block).await, "write");
  }
  ok(file.flush().await, "flush");

  let written = ok(fs::read(&path).await, "read");
  assert_eq!(written.len(), block.len() * 5);
  assert!(written.iter().all(|byte| *byte == 7));
}

#[wasm_bindgen_test]
async fn reports_the_kinds_that_tokio_reports() {
  let directory = scratch("error_kinds").await;
  let file = directory.join("file.txt");
  ok(fs::write(&file, b"x").await, "write");
  let nested = directory.join("nested");
  ok(fs::create_dir(&nested).await, "create");
  ok(fs::write(nested.join("held.txt"), b"x").await, "write");

  assert_eq!(kind(fs::read(&nested).await), ErrorKind::IsADirectory);
  assert_eq!(
    kind(fs::remove_file(&nested).await),
    ErrorKind::IsADirectory
  );
  assert_eq!(kind(fs::read_dir(&file).await), ErrorKind::NotADirectory);
  assert_eq!(kind(fs::remove_dir(&file).await), ErrorKind::NotADirectory);
  assert_eq!(
    kind(fs::remove_dir(&nested).await),
    ErrorKind::DirectoryNotEmpty
  );
}

#[wasm_bindgen_test]
async fn a_file_sitting_on_the_name_counts_as_taken() {
  let directory = scratch("name_taken").await;
  let path = directory.join("taken");
  ok(fs::write(&path, b"x").await, "write");
  assert_eq!(kind(fs::create_dir(&path).await), ErrorKind::AlreadyExists);
}

#[wasm_bindgen_test]
async fn metadata_still_reads_a_directory() {
  let directory = scratch("metadata_kinds").await;
  let nested = directory.join("nested");
  ok(fs::create_dir(&nested).await, "create");
  assert!(ok(fs::metadata(&nested).await, "read metadata").is_dir());
  assert!(ok(fs::try_exists(&nested).await, "look"));
  assert!(!ok(fs::try_exists(directory.join("absent")).await, "look"));
}

#[wasm_bindgen_test]
async fn a_flush_partway_through_keeps_the_rest_in_order() {
  let directory = scratch("flush_partway").await;
  let path = directory.join("ordered.bin");
  let block = vec![3u8; 400 * 1024];

  let mut file = ok(fs::File::create(&path).await, "create");
  ok(file.write_all(&block).await, "write");
  ok(file.write_all(&block).await, "write");
  ok(file.flush().await, "flush partway");
  assert_eq!(ok(fs::read(&path).await, "read").len(), block.len() * 2);

  ok(file.write_all(&block).await, "write after the flush");
  ok(file.flush().await, "flush");

  let written = ok(fs::read(&path).await, "read");
  assert_eq!(written.len(), block.len() * 3);
  assert!(written.iter().all(|byte| *byte == 3));
}

#[wasm_bindgen_test]
async fn appending_lands_past_what_arrived_in_between() {
  let directory = scratch("append_follows").await;
  let path = directory.join("log.txt");
  ok(fs::write(&path, b"first\n").await, "write");

  let mut file = ok(
    fs::File::options().append(true).open(&path).await,
    "open for appending",
  );
  ok(file.write_all(b"second\n").await, "write");
  ok(file.flush().await, "flush");

  // Something else adds to the file while nothing of ours is open.
  let mut other = ok(
    fs::File::options().append(true).open(&path).await,
    "open for appending",
  );
  ok(other.write_all(b"third\n").await, "write");
  ok(other.flush().await, "flush");
  drop(other);

  // The first file has to land past it rather than on top of it.
  ok(file.write_all(b"fourth\n").await, "write");
  ok(file.flush().await, "flush");

  assert_eq!(
    ok(fs::read_to_string(&path).await, "read"),
    "first\nsecond\nthird\nfourth\n"
  );
}

#[wasm_bindgen_test]
async fn a_file_still_works_after_a_write_is_cancelled() {
  let directory = scratch("cancelled_write").await;
  let path = directory.join("cancelled.bin");
  let block = vec![9u8; 2 * 1024 * 1024];

  let mut file = ok(fs::File::create(&path).await, "create");
  // The first write only fills the buffer. The second one pushes it, which
  // is the call that a cancellation can leave in flight.
  ok(file.write_all(&block).await, "write");
  let _ = timeout(Duration::from_millis(1), file.write_all(&block)).await;
  ok(file.sync_all().await, "sync");

  // Whatever the cancellation left behind, the file has to be closed and
  // the handle usable, so that what comes next lands where it belongs.
  let settled = ok(fs::metadata(&path).await, "read metadata").len();
  ok(
    file.write_all(b"tail").await,
    "write after the cancellation",
  );
  ok(file.flush().await, "flush");

  let written = ok(fs::read(&path).await, "read");
  assert!(
    written.len() >= settled as usize,
    "the file shrank: {settled} then {}",
    written.len()
  );
  assert!(
    written.ends_with(b"tail"),
    "the write after the cancellation did not land"
  );
}

#[wasm_bindgen_test]
async fn reads_the_same_bytes_however_small_the_asking_buffer() {
  let directory = scratch("read_ahead").await;
  let path = directory.join("read_ahead.bin");
  // Longer than one read ahead, so more than one round trip is needed.
  let written: Vec<u8> =
    (0..700 * 1024).map(|index| (index % 251) as u8).collect();
  ok(fs::write(&path, &written).await, "write");

  let mut file = ok(fs::File::open(&path).await, "open");
  let mut read = Vec::new();
  let mut chunk = [0u8; 8192];
  loop {
    let count = ok(file.read(&mut chunk).await, "read");
    if count == 0 {
      break;
    }
    read.extend_from_slice(&chunk[..count]);
  }
  assert_eq!(read.len(), written.len());
  assert!(read == written, "the bytes came back changed");
}

#[wasm_bindgen_test]
async fn a_write_throws_away_what_was_read_ahead() {
  let directory = scratch("read_ahead_stale").await;
  let path = directory.join("stale.txt");
  ok(fs::write(&path, b"0123456789").await, "write");

  let mut file = ok(
    fs::File::options().read(true).write(true).open(&path).await,
    "open",
  );
  let mut head = [0u8; 2];
  ok(file.read_exact(&mut head).await, "read");
  assert_eq!(&head, b"01");

  // This lands at the cursor and invalidates the rest of the read ahead.
  ok(file.write_all(b"XY").await, "write");
  ok(file.flush().await, "flush");
  ok(file.seek(SeekFrom::Start(0)).await, "seek");

  let mut all = String::new();
  ok(file.read_to_string(&mut all).await, "read");
  assert_eq!(all, "01XY456789");
}
