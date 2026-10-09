//! Every test works inside a directory of its own, because the origin
//! private file system outlives the test that wrote to it.

use super::{cancel, spy};
use std::future::Future;
use std::io::{ErrorKind, Result, SeekFrom};
use std::path::{Path, PathBuf};
use std::task::{Context, Waker};
use tokio_with_wasm::fs;
use tokio_with_wasm::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_with_wasm::time::{Duration, sleep};
use wasm_bindgen_test::wasm_bindgen_test;

/// Hands back an empty directory, clearing what an earlier run left.
async fn scratch(name: &str) -> Result<PathBuf> {
  match fs::remove_dir_all(name).await {
    Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
    _ => fs::create_dir(name).await?,
  }
  Ok(PathBuf::from(name))
}

/// Bytes that give away a chunk written twice or out of order.
///
/// The stride is prime, so it never lines up with a buffer boundary the
/// way a round number would.
fn pattern(length: usize) -> Vec<u8> {
  (0..length).map(|index| (index % 251) as u8).collect()
}

/// Waits for a commit handed to the event loop, which signals nothing.
async fn wait_for_size(path: &Path, size: u64) {
  for _ in 0..200 {
    if fs::metadata(path).await.unwrap().len() == size {
      return;
    }
    sleep(Duration::from_millis(10)).await;
  }
}

/// Reads the kind off a call that was supposed to fail.
fn kind<T>(result: Result<T>) -> ErrorKind {
  result.map(drop).unwrap_err().kind()
}

#[wasm_bindgen_test]
async fn writes_and_reads_a_file_back() -> Result<()> {
  let directory = scratch("writes_and_reads").await?;
  let path = directory.join("greeting.txt");

  fs::write(&path, b"the long first draft").await?;
  fs::write(&path, b"hello").await?;
  assert_eq!(fs::read(&path).await?, b"hello");
  assert_eq!(fs::read_to_string(&path).await?, "hello");
  Ok(())
}

#[wasm_bindgen_test]
async fn a_leading_slash_names_the_same_file() -> Result<()> {
  let directory = scratch("leading_slash").await?;
  fs::write(directory.join("same.txt"), b"once").await?;

  let absolute = PathBuf::from("/").join(&directory).join("same.txt");
  assert_eq!(fs::read_to_string(absolute).await?, "once");
  Ok(())
}

#[wasm_bindgen_test]
async fn a_path_cannot_lead_out_of_the_root() {
  assert_eq!(kind(fs::read("../secret").await), ErrorKind::InvalidInput);
}

#[wasm_bindgen_test]
async fn reports_what_it_knows_about_an_entry() -> Result<()> {
  let directory = scratch("metadata").await?;
  let path = directory.join("sized.bin");
  fs::write(&path, b"12345").await?;

  let about_file = fs::metadata(&path).await?;
  assert!(about_file.is_file());
  assert!(!about_file.is_dir());
  assert_eq!(about_file.len(), 5);
  assert!(about_file.modified().is_ok());

  let about_directory = fs::metadata(&directory).await?;
  assert!(about_directory.is_dir());
  assert!(!about_directory.is_file());
  assert!(about_directory.modified().is_err());
  Ok(())
}

#[wasm_bindgen_test]
async fn lists_what_a_directory_holds() -> Result<()> {
  let directory = scratch("read_dir").await?;
  fs::write(directory.join("a.txt"), b"a").await?;
  fs::write(directory.join("b.txt"), b"bb").await?;
  fs::create_dir(directory.join("inner")).await?;

  let mut found = Vec::new();
  let mut entries = fs::read_dir(&directory).await?;
  // A cancelled step must not lose an entry.
  cancel(entries.next_entry());
  while let Some(entry) = entries.next_entry().await? {
    let name = entry.file_name().to_string_lossy().into_owned();
    assert_eq!(entry.path(), PathBuf::from("/read_dir").join(&name));
    let kind = entry.file_type().await?;
    let length = entry.metadata().await?.len();
    found.push((name, kind.is_dir(), length));
  }
  found.sort();

  assert_eq!(
    found,
    [
      ("a.txt".to_owned(), false, 1),
      ("b.txt".to_owned(), false, 2),
      ("inner".to_owned(), true, 0),
    ]
  );
  Ok(())
}

#[wasm_bindgen_test]
async fn creating_a_directory_twice_reports_it_is_there() -> Result<()> {
  let directory = scratch("create_dir_twice").await?;
  let inner = directory.join("once");
  fs::create_dir(&inner).await?;
  assert_eq!(kind(fs::create_dir(&inner).await), ErrorKind::AlreadyExists);
  let file = directory.join("taken");
  fs::write(&file, b"x").await?;
  assert_eq!(kind(fs::create_dir(&file).await), ErrorKind::AlreadyExists);
  Ok(())
}

#[wasm_bindgen_test]
async fn removes_files_and_directories() -> Result<()> {
  let directory = scratch("removing").await?;
  let file = directory.join("gone.txt");
  fs::write(&file, b"x").await?;
  fs::remove_file(&file).await?;
  assert!(!fs::try_exists(&file).await?);

  let empty = directory.join("empty");
  fs::create_dir(&empty).await?;
  assert!(fs::try_exists(&empty).await?);
  fs::remove_dir(&empty).await?;
  assert!(!fs::try_exists(&empty).await?);
  Ok(())
}

#[wasm_bindgen_test]
async fn renames_a_directory_and_everything_under_it() -> Result<()> {
  let directory = scratch("renaming_directory").await?;
  let from = directory.join("before");
  let to = directory.join("after");
  fs::create_dir_all(from.join("deep")).await?;
  fs::write(from.join("top.txt"), b"top").await?;
  fs::write(from.join("deep/low.txt"), b"low").await?;

  fs::rename(&from, &to).await?;
  assert_eq!(fs::read_to_string(to.join("top.txt")).await?, "top");
  assert_eq!(fs::read_to_string(to.join("deep/low.txt")).await?, "low");
  assert!(!fs::try_exists(&from).await?);

  fs::rename(&to, &to).await?;
  assert_eq!(
    kind(fs::rename(&to, to.join("deep/inner")).await),
    ErrorKind::InvalidInput
  );
  fs::create_dir(&from).await?;
  assert_eq!(
    kind(fs::rename(&from, &to).await),
    ErrorKind::DirectoryNotEmpty
  );
  assert_eq!(fs::read_to_string(to.join("deep/low.txt")).await?, "low");
  Ok(())
}

#[wasm_bindgen_test]
async fn spells_a_path_one_way() -> Result<()> {
  let directory = scratch("canonicalize").await?;
  fs::write(directory.join("here.txt"), b"x").await?;

  // `..` is resolved without walking, because there is nothing to follow.
  // `tokio` would refuse this path, where `inner` is not there at all.
  let spelled = fs::canonicalize("canonicalize/./inner/../here.txt").await?;
  assert_eq!(spelled, PathBuf::from("/canonicalize/here.txt"));

  // What the entry leads to still has to be there, as in `tokio`.
  assert_eq!(
    kind(fs::canonicalize(directory.join("absent")).await),
    ErrorKind::NotFound
  );
  Ok(())
}

#[wasm_bindgen_test]
async fn writes_through_an_open_file() -> Result<()> {
  let directory = scratch("open_write").await?;
  let path = directory.join("streamed.txt");
  fs::write(&path, b"the older and longer one").await?;

  let mut file = fs::File::create(&path).await?;
  file.write_all(b"first ").await?;
  file.write_all(b"second").await?;
  // Not carrying on from the held back bytes, so they have to go out first.
  file.seek(SeekFrom::Start(1)).await?;
  file.write_all(b"X").await?;
  file.flush().await?;

  assert_eq!(fs::read_to_string(&path).await?, "fXrst second");
  Ok(())
}

#[wasm_bindgen_test]
async fn dropping_a_file_still_lands_the_writes() -> Result<()> {
  let directory = scratch("dropped_write").await?;
  let path = directory.join("dropped.txt");

  let mut file = fs::File::create(&path).await?;
  file.write_all(b"left behind").await?;
  drop(file);
  wait_for_size(&path, 11).await;
  assert_eq!(fs::read_to_string(&path).await?, "left behind");

  // A stream left open with nothing held back is closed too.
  let mut file = fs::File::create(&path).await?;
  file.write_all(&pattern(1 << 20)).await?;
  assert_eq!(file.write(&[]).await?, 0);
  drop(file);
  wait_for_size(&path, 1 << 20).await;
  assert_eq!(fs::read(&path).await?, pattern(1 << 20));
  Ok(())
}

#[wasm_bindgen_test]
async fn dropping_a_file_mid_push_still_lands_the_writes() -> Result<()> {
  let directory = scratch("dropped_push").await?;
  let path = directory.join("dropped.bin");
  let block = vec![9u8; 2 * 1024 * 1024];

  let mut file = fs::File::create(&path).await?;
  file.write_all(&block).await?;
  {
    // The first poll opens the stream and the second, once it is open,
    // starts pushing the buffer, which no poll can finish at once.
    let mut flushing = Box::pin(file.flush());
    let mut context = Context::from_waker(Waker::noop());
    assert!(flushing.as_mut().poll(&mut context).is_pending());
    sleep(Duration::from_millis(100)).await;
    assert!(flushing.as_mut().poll(&mut context).is_pending());
  }
  drop(file);
  wait_for_size(&path, block.len() as u64).await;
  assert_eq!(fs::read(&path).await?.len(), block.len());
  Ok(())
}

#[wasm_bindgen_test]
async fn a_cancelled_read_does_not_answer_a_later_one() -> Result<()> {
  let directory = scratch("cancelled_read").await?;
  let path = directory.join("cancelled.txt");
  fs::write(&path, b"0123456789").await?;

  let mut file = fs::File::options()
    .read(true)
    .write(true)
    .open(&path)
    .await?;
  let mut head = [0u8; 2];
  cancel(file.read(&mut head));
  file.seek(SeekFrom::Start(5)).await?;
  let mut rest = String::new();
  file.read_to_string(&mut rest).await?;
  assert_eq!(rest, "56789");

  file.rewind().await?;
  cancel(file.read(&mut head));
  file.write_all(b"AB").await?;
  rest.clear();
  file.read_to_string(&mut rest).await?;
  assert_eq!(rest, "23456789");
  Ok(())
}

#[wasm_bindgen_test]
async fn a_read_sees_what_was_just_written() -> Result<()> {
  let directory = scratch("write_then_read").await?;
  let path = directory.join("both.txt");

  let mut file = fs::File::options()
    .read(true)
    .write(true)
    .create(true)
    .truncate(true)
    .open(&path)
    .await?;
  file.write_all(b"abcdef").await?;
  file.seek(SeekFrom::Start(2)).await?;

  let mut read = Vec::new();
  file.read_to_end(&mut read).await?;
  assert_eq!(read, b"cdef");
  Ok(())
}

#[wasm_bindgen_test]
async fn seeks_from_every_side() -> Result<()> {
  let directory = scratch("seeking").await?;
  let path = directory.join("seek.txt");
  fs::write(&path, b"0123456789").await?;

  let mut file = fs::File::open(&path).await?;
  assert_eq!(file.seek(SeekFrom::Start(4)).await?, 4);
  assert_eq!(file.seek(SeekFrom::Current(2)).await?, 6);
  assert_eq!(file.seek(SeekFrom::End(-3)).await?, 7);

  let mut read = Vec::new();
  file.read_to_end(&mut read).await?;
  assert_eq!(read, b"789");

  // A cancelled seek still lands before the next read, as in `tokio`.
  file.rewind().await?;
  cancel(file.seek(SeekFrom::End(-2)));
  read.clear();
  file.read_to_end(&mut read).await?;
  assert_eq!(read, b"89");
  Ok(())
}

#[wasm_bindgen_test]
async fn seeking_before_the_start_fails_and_is_over() -> Result<()> {
  let directory = scratch("seek_before_start").await?;
  let path = directory.join("short.txt");
  fs::write(&path, b"ab").await?;

  let mut file = fs::File::open(&path).await?;
  assert_eq!(
    kind(file.seek(SeekFrom::Current(-1)).await),
    ErrorKind::InvalidInput
  );
  // Landing before the start is only discovered after the file
  // has been measured, unlike the `SeekFrom::Current` case.
  assert_eq!(
    kind(file.seek(SeekFrom::End(-3)).await),
    ErrorKind::InvalidInput
  );

  // The failed seek must not linger: the next seek starts fresh
  // instead of reporting that a seek is already under way.
  assert_eq!(file.seek(SeekFrom::Start(1)).await?, 1);
  let mut read = Vec::new();
  file.read_to_end(&mut read).await?;
  assert_eq!(read, b"b");
  Ok(())
}

#[wasm_bindgen_test]
async fn cuts_a_file_down_to_size() -> Result<()> {
  let directory = scratch("set_len").await?;
  let path = directory.join("trimmed.txt");
  fs::write(&path, b"0123456789").await?;

  let mut file = fs::File::options()
    .read(true)
    .write(true)
    .open(&path)
    .await?;
  let mut head = [0u8; 2];
  file.read_exact(&mut head).await?;
  file.set_len(4).await?;
  let mut rest = String::new();
  file.read_to_string(&mut rest).await?;
  assert_eq!(rest, "23");
  file.set_len(6).await?;
  assert_eq!(fs::read(&path).await?, b"0123\0\0");
  Ok(())
}

#[wasm_bindgen_test]
async fn appending_writes_at_the_end() -> Result<()> {
  let directory = scratch("appending").await?;
  let path = directory.join("log.txt");
  fs::write(&path, b"first\n").await?;

  let mut file = fs::File::options()
    .read(true)
    .append(true)
    .open(&path)
    .await?;
  // Only writes go to the end; reading starts at the start.
  let mut read = String::new();
  file.read_to_string(&mut read).await?;
  assert_eq!(read, "first\n");
  file.write_all(b"second\n").await?;
  // Appending ignores the cursor, held back bytes or not.
  file.seek(SeekFrom::Start(0)).await?;
  file.write_all(b"third\n").await?;
  file.flush().await?;

  assert_eq!(fs::read_to_string(&path).await?, "first\nsecond\nthird\n");
  Ok(())
}

#[wasm_bindgen_test]
async fn appending_keeps_one_stream_open() -> Result<()> {
  let directory = scratch("appending_stream").await?;
  let path = directory.join("log.bin");
  fs::write(&path, b"").await?;
  let mut file = fs::File::options().append(true).open(&path).await?;

  // Opening a stream copies the whole file, so one per chunk would make
  // appending quadratic. This counts the streams that get opened.
  let chunk = pattern(1 << 20);
  let opened = spy(
    "const proto = FileSystemFileHandle.prototype;
     const open = proto.createWritable;
     let opened = 0;
     proto.createWritable = function (...args) {
       opened += 1;
       return open.apply(this, args);
     };
     return () => { proto.createWritable = open; return opened; };",
    async {
      for _ in 0..4 {
        file.write_all(&chunk).await.unwrap();
      }
      file.flush().await.unwrap();
    },
  )
  .await;
  assert_eq!(opened.as_f64(), Some(1.0), "streams were reopened");
  assert_eq!(fs::metadata(&path).await?.len(), 4 << 20);
  Ok(())
}

#[wasm_bindgen_test]
async fn creating_a_new_file_over_an_old_one_does_not_work() -> Result<()> {
  let directory = scratch("create_new").await?;
  let path = directory.join("once.txt");

  fs::write(&path, b"kept").await?;
  assert_eq!(
    kind(fs::File::create_new(&path).await),
    ErrorKind::AlreadyExists
  );
  assert_eq!(fs::read_to_string(&path).await?, "kept");

  fs::create_dir(directory.join("taken")).await?;
  assert_eq!(
    kind(
      fs::File::options()
        .write(true)
        .create_new(true)
        .open(directory.join("taken"))
        .await
    ),
    ErrorKind::AlreadyExists
  );
  Ok(())
}

#[wasm_bindgen_test]
async fn a_file_only_does_what_it_was_opened_for() -> Result<()> {
  let directory = scratch("open_modes").await?;
  let path = directory.join("locked.txt");
  fs::write(&path, b"x").await?;

  let mut file = fs::File::open(&path).await?;
  assert_eq!(
    kind(file.write_all(b"y").await),
    ErrorKind::PermissionDenied
  );
  let opened = fs::File::options().open(&path).await;
  assert_eq!(kind(opened), ErrorKind::InvalidInput);
  Ok(())
}

#[wasm_bindgen_test]
async fn writes_more_than_the_buffer_holds() -> Result<()> {
  let directory = scratch("large_write").await?;
  let path = directory.join("large.bin");
  let written = pattern(5 * 300 * 1024);

  let mut file = fs::File::create(&path).await?;
  for slice in written.chunks(300 * 1024) {
    file.write_all(slice).await?;
  }
  file.flush().await?;

  let read = fs::read(&path).await?;
  assert_eq!(read.len(), written.len());
  assert!(read == written, "the bytes came back out of order");
  Ok(())
}

#[wasm_bindgen_test]
async fn reports_the_kinds_that_tokio_reports() -> Result<()> {
  let directory = scratch("error_kinds").await?;
  let file = directory.join("file.txt");
  fs::write(&file, b"x").await?;
  let nested = directory.join("nested");
  fs::create_dir(&nested).await?;
  fs::write(nested.join("held.txt"), b"x").await?;

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
  assert!(fs::try_exists(&file).await?);
  fs::remove_dir_all(&nested).await?;
  assert!(!fs::try_exists(&nested).await?);
  Ok(())
}

#[wasm_bindgen_test]
async fn a_flush_partway_through_keeps_the_rest_in_order() -> Result<()> {
  let directory = scratch("flush_partway").await?;
  let path = directory.join("ordered.bin");
  let block = 400 * 1024;
  let written = pattern(3 * block);

  let mut file = fs::File::create(&path).await?;
  file.write_all(&written[..block]).await?;
  file.write_all(&written[block..block * 2]).await?;
  file.flush().await?;
  assert!(
    fs::read(&path).await? == written[..block * 2],
    "the flush partway did not leave the first two blocks in order"
  );

  file.write_all(&written[block * 2..]).await?;
  file.flush().await?;

  let read = fs::read(&path).await?;
  assert_eq!(read.len(), written.len());
  assert!(read == written, "the bytes came back out of order");
  Ok(())
}

#[wasm_bindgen_test]
async fn appending_lands_past_what_arrived_in_between() -> Result<()> {
  let directory = scratch("append_follows").await?;
  let path = directory.join("log.txt");
  fs::write(&path, b"first\n").await?;

  let mut file = fs::File::options().append(true).open(&path).await?;
  file.write_all(b"second\n").await?;
  file.flush().await?;

  // Something else adds to the file while nothing of ours is open.
  let mut other = fs::File::options().append(true).open(&path).await?;
  other.write_all(b"third\n").await?;
  other.flush().await?;
  drop(other);

  // The first file has to land past it rather than on top of it.
  file.write_all(b"fourth\n").await?;
  file.flush().await?;

  assert_eq!(
    fs::read_to_string(&path).await?,
    "first\nsecond\nthird\nfourth\n"
  );
  Ok(())
}

#[wasm_bindgen_test]
async fn a_file_still_works_after_a_write_is_cancelled() -> Result<()> {
  let directory = scratch("cancelled_write").await?;
  let path = directory.join("cancelled.bin");
  let block = vec![9u8; 2 * 1024 * 1024];

  let mut file = fs::File::create(&path).await?;
  // The first write only fills the buffer. The second one has to push it,
  // and that push cannot finish inside a single poll, so dropping the
  // future right after leaves the call in flight for `sync_all` to find.
  file.write_all(&block).await?;
  cancel(file.write_all(b"dropped"));
  file.sync_all().await?;

  // Whatever the cancellation left behind, the file has to be closed and
  // the handle usable, so that what comes next lands where it belongs.
  let settled = fs::metadata(&path).await?.len();
  assert_eq!(settled as usize, block.len(), "the buffer was stranded");
  file.write_all(b"tail").await?;
  file.flush().await?;

  let written = fs::read(&path).await?;
  assert!(
    written.len() >= settled as usize,
    "the file shrank: {settled} then {}",
    written.len()
  );
  assert!(
    written.ends_with(b"tail"),
    "the write after the cancellation did not land"
  );
  Ok(())
}

#[wasm_bindgen_test]
async fn reads_the_same_bytes_however_small_the_asking_buffer() -> Result<()> {
  let directory = scratch("read_ahead").await?;
  let path = directory.join("read_ahead.bin");
  // Longer than one read ahead, so more than one round trip is needed.
  let written = pattern(700 * 1024);
  fs::write(&path, &written).await?;

  let mut file = fs::File::open(&path).await?;
  let mut read = Vec::new();
  let mut chunk = [0u8; 8192];
  loop {
    let count = file.read(&mut chunk).await?;
    if count == 0 {
      break;
    }
    read.extend_from_slice(&chunk[..count]);
  }
  assert_eq!(read.len(), written.len());
  assert!(read == written, "the bytes came back changed");
  Ok(())
}

#[wasm_bindgen_test]
async fn a_write_throws_away_what_was_read_ahead() -> Result<()> {
  let directory = scratch("read_ahead_stale").await?;
  let path = directory.join("stale.txt");
  fs::write(&path, b"0123456789").await?;

  let mut file = fs::File::options()
    .read(true)
    .write(true)
    .open(&path)
    .await?;
  let mut head = [0u8; 2];
  file.read_exact(&mut head).await?;
  assert_eq!(&head, b"01");

  // This lands at the cursor and invalidates the rest of the read ahead.
  file.write_all(b"XY").await?;
  file.flush().await?;
  file.seek(SeekFrom::Start(0)).await?;

  let mut all = String::new();
  file.read_to_string(&mut all).await?;
  assert_eq!(all, "01XY456789");
  Ok(())
}

#[wasm_bindgen_test]
async fn two_open_files_that_flush_in_turn_both_land() -> Result<()> {
  let directory = scratch("two_handles_small").await?;
  let path = directory.join("shared.txt");
  fs::write(&path, b"..........").await?;

  let mut first = fs::File::options().write(true).open(&path).await?;
  let mut second = fs::File::options().write(true).open(&path).await?;

  first.write_all(b"AA").await?;
  second.seek(SeekFrom::Start(5)).await?;
  second.write_all(b"BB").await?;
  first.flush().await?;
  second.flush().await?;

  assert_eq!(fs::read_to_string(&path).await?, "AA...BB...");
  Ok(())
}

#[wasm_bindgen_test]
async fn two_open_files_that_overlap_keep_only_the_last_to_close() -> Result<()>
{
  let directory = scratch("two_handles_large").await?;
  let path = directory.join("shared.bin");
  fs::write(&path, b"").await?;

  let mut first = fs::File::options().write(true).open(&path).await?;
  let mut second = fs::File::options().write(true).open(&path).await?;

  // Enough to spill, which is what makes a file hold its stream open
  // rather than opening and closing one inside a single flush.
  let mine = vec![b'A'; 2 * 1024 * 1024];
  let yours = vec![b'B'; 2 * 1024 * 1024];
  first.write_all(&mine).await?;
  first.write_all(b"!").await?;
  second.write_all(&yours).await?;
  second.write_all(b"!").await?;
  first.flush().await?;
  second.flush().await?;

  let written = fs::read(&path).await?;
  assert_eq!(written.len(), yours.len() + 1);
  assert!(
    written.iter().take(yours.len()).all(|byte| *byte == b'B'),
    "the file should hold what the last one to close wrote"
  );
  Ok(())
}

#[wasm_bindgen_test]
async fn a_file_reports_and_syncs_through_its_own_handle() -> Result<()> {
  let directory = scratch("file_handle_api").await?;
  let path = directory.join("through.txt");

  let mut file = fs::File::create_new(&path).await?;
  file.write_all(b"held").await?;
  assert_eq!(file.metadata().await?.len(), 0);
  file.sync_data().await?;

  let about = file.metadata().await?;
  assert_eq!(about.len(), 4);
  assert!(about.is_file());
  assert!(about.file_type().is_file());
  assert!(!about.is_symlink());
  assert!(!about.file_type().is_symlink());
  Ok(())
}

#[wasm_bindgen_test]
async fn text_that_is_not_utf8_is_refused() -> Result<()> {
  let directory = scratch("not_utf8").await?;
  let path = directory.join("bytes.bin");
  fs::write(&path, [0xff, 0xfe]).await?;

  assert_eq!(
    kind(fs::read_to_string(&path).await),
    ErrorKind::InvalidData
  );
  assert_eq!(fs::read(&path).await?, [0xff, 0xfe]);
  Ok(())
}

#[wasm_bindgen_test]
async fn copying_and_renaming_replace_what_is_already_there() -> Result<()> {
  let directory = scratch("replacing").await?;
  let from = directory.join("from.txt");
  let onto = directory.join("onto.txt");
  fs::write(&from, b"new").await?;
  fs::write(&onto, b"the older and longer one").await?;

  assert_eq!(fs::copy(&from, &onto).await?, 3);
  assert_eq!(fs::read_to_string(&onto).await?, "new");
  assert!(fs::try_exists(&from).await?);

  fs::write(&onto, b"the older and longer one").await?;
  fs::rename(&from, &onto).await?;
  assert_eq!(fs::read_to_string(&onto).await?, "new");
  assert!(!fs::try_exists(&from).await?);
  fs::rename(&onto, &onto).await?;
  assert_eq!(fs::read_to_string(&onto).await?, "new");
  Ok(())
}

#[wasm_bindgen_test]
async fn an_empty_file_reads_as_nothing() -> Result<()> {
  let directory = scratch("empty_file").await?;
  let path = directory.join("empty.bin");
  fs::write(&path, b"").await?;

  assert_eq!(fs::read(&path).await?, Vec::<u8>::new());
  let mut file = fs::File::open(&path).await?;
  let mut read = Vec::new();
  assert_eq!(file.read_to_end(&mut read).await?, 0);
  Ok(())
}

#[wasm_bindgen_test]
async fn removing_what_is_not_there_reports_not_found() -> Result<()> {
  let directory = scratch("removing_absent").await?;
  let absent = directory.join("absent");
  assert_eq!(kind(fs::read(&absent).await), ErrorKind::NotFound);
  assert_eq!(kind(fs::remove_file(&absent).await), ErrorKind::NotFound);
  assert_eq!(kind(fs::remove_dir(&absent).await), ErrorKind::NotFound);
  assert_eq!(kind(fs::remove_dir_all(&absent).await), ErrorKind::NotFound);
  assert_eq!(kind(fs::read_dir(&absent).await), ErrorKind::NotFound);
  Ok(())
}

#[wasm_bindgen_test]
async fn writing_past_the_end_leaves_zeros_in_the_gap() -> Result<()> {
  let directory = scratch("gap").await?;
  let path = directory.join("gap.bin");
  fs::write(&path, b"ab").await?;

  let mut file = fs::File::options().write(true).open(&path).await?;
  // A cancelled seek still lands before the next write, as in `tokio`.
  cancel(file.seek(SeekFrom::End(3)));
  file.write_all(b"z").await?;
  file.flush().await?;

  assert_eq!(fs::read(&path).await?, [b'a', b'b', 0, 0, 0, b'z']);
  Ok(())
}
