//! Files in the origin private file system.

use super::dir::{Metadata, file_metadata};
use super::error::{Wanted, await_call, await_js, cast};
use super::handle::file_at;
use crate::error;
use js_sys::Uint8Array;
use std::future::{Future, poll_fn};
use std::io::{self, SeekFrom};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
  FileSystemCreateWritableOptions, FileSystemFileHandle,
  FileSystemWritableFileStream,
};

/// How many bytes a file holds in memory before pushing them to the stream.
const WRITE_BUFFER_LIMIT: usize = 1 << 20;

/// How much a file reads at once, however little was asked for.
///
/// Every read is a round trip through JavaScript, so a run of small reads
/// would otherwise pay for one each. Reading a whole file eight kilobytes
/// at a time costs a hundred and twenty eight times fewer of them this way.
const READ_AHEAD: usize = 256 * 1024;

/// A call into JavaScript that has been started but has not finished.
type Started<T> = Pin<Box<dyn Future<Output = io::Result<T>>>>;

/// The stream that carries writes, and where its own cursor sits.
struct Writer {
  stream: FileSystemWritableFileStream,
  cursor: u64,
}

/// The one call a file can have in flight.
enum Work {
  Reading(Started<Vec<u8>>),
  Opening(Started<FileSystemWritableFileStream>),
  Pushing(Started<Writer>),
  Closing(Started<()>),
  Sizing(Started<u64>),
}

/// A seek that has been asked for but not reported back yet.
enum Seek {
  /// The new position is already known.
  Settled(u64),
  /// The new position is that many bytes from the end of the file.
  FromEnd(i64),
}

/// An open file in the origin private file system.
///
/// Writes reach the file only once it is flushed, shut down, or dropped,
/// because the web API commits a write when the stream carrying it closes.
/// `flush` is what makes them visible to a later read, and so it matters
/// more here than in `tokio`, where the operating system takes writes as
/// they come.
///
/// Opening that stream copies the whole file, so one stream is kept open
/// across writes rather than opened per chunk. A flush per chunk brings the
/// copy back, once for each flush.
///
/// Two open files on one path never share it. Each writes into the copy it
/// took when its stream opened, and closing that stream replaces the whole
/// file. A stream is only open from the moment writes spill out of memory
/// until the next flush, so two files lose each other's work only when
/// those windows overlap:
///
/// |                              | `tokio`     | here                   |
/// | ---------------------------- | ----------- | ---------------------- |
/// | flushes that do not overlap  | both land   | both land              |
/// | overlapping, other offsets   | both land   | only the last to close |
/// | overlapping, same offset     | interleaved | only the last to close |
/// | crash partway through        | half a file | nothing half written   |
///
/// So the file is never left holding something no writer wrote, and a write
/// that `tokio` would have kept can still be lost. Nothing on the main
/// thread closes that gap: the web API only hands out the handle that
/// writes in place, `createSyncAccessHandle`, inside a worker.
pub struct File {
  handle: FileSystemFileHandle,
  /// Where the next read or write lands.
  position: u64,
  /// Bytes written but not yet pushed to the stream.
  buffer: Vec<u8>,
  /// Where the first byte of `buffer` belongs in the file.
  buffer_start: u64,
  /// Bytes read ahead of what was asked for.
  read_buffer: Vec<u8>,
  /// Where the first byte of `read_buffer` sits in the file.
  read_start: u64,
  writer: Option<Writer>,
  work: Option<Work>,
  seek: Option<Seek>,
  readable: bool,
  writable: bool,
  /// Whether every write goes to the end rather than to the cursor.
  appending: bool,
}

impl File {
  /// Opens a file for reading.
  pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
    OpenOptions::new().read(true).open(path).await
  }

  /// Opens a file for writing, creating it if it is missing
  /// and emptying it if it is not.
  pub async fn create(path: impl AsRef<Path>) -> io::Result<File> {
    OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(true)
      .open(path)
      .await
  }

  /// Opens a file for writing, and fails if it is already there.
  pub async fn create_new(path: impl AsRef<Path>) -> io::Result<File> {
    OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(path)
      .await
  }

  /// Starts building the options to open a file with.
  pub fn options() -> OpenOptions {
    OpenOptions::new()
  }

  /// Wraps a handle that has already been looked up.
  fn from_handle(handle: FileSystemFileHandle) -> File {
    File {
      handle,
      position: 0,
      buffer: Vec::new(),
      buffer_start: 0,
      read_buffer: Vec::new(),
      read_start: 0,
      writer: None,
      work: None,
      seek: None,
      readable: true,
      writable: true,
      appending: false,
    }
  }

  /// Reports what is known about the file.
  ///
  /// Bytes that have not been flushed do not count towards the size yet.
  pub async fn metadata(&self) -> io::Result<Metadata> {
    file_metadata(&self.handle).await
  }

  /// Grows or shrinks the file to `size` bytes.
  pub async fn set_len(&mut self, size: u64) -> io::Result<()> {
    self.settle().await?;
    self.forget_read_ahead();
    let stream = open_writer(self.handle.clone(), true).await?;
    await_call(stream.truncate_with_f64(size as f64), Wanted::File).await?;
    close_writer(stream).await
  }

  /// Writes every held back byte into the file.
  pub async fn sync_all(&mut self) -> io::Result<()> {
    self.settle().await
  }

  /// Writes every held back byte into the file.
  ///
  /// The web API draws no line between a file's contents and what is known
  /// about it, so this does the same as [`File::sync_all`].
  pub async fn sync_data(&mut self) -> io::Result<()> {
    self.settle().await
  }

  /// Pushes and closes, from outside a `poll` function.
  ///
  /// This goes through the same state machine the `poll` functions use
  /// rather than repeating it. A write that was cancelled halfway leaves a
  /// call in flight, and only that machine knows to pick it back up; doing
  /// the work here again would strand it and report success.
  async fn settle(&mut self) -> io::Result<()> {
    poll_fn(|cx| self.poll_closed(cx)).await
  }

  /// Drives a call of a kind the caller did not ask for.
  ///
  /// A read or a measurement that is dropped this way never reported
  /// anything and never moved the cursor, so letting go of its answer only
  /// means it did not happen.
  fn poll_settled(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    match self.work.take() {
      None => Poll::Ready(Ok(())),
      Some(Work::Reading(mut future)) => match future.as_mut().poll(cx) {
        Poll::Pending => {
          self.work = Some(Work::Reading(future));
          Poll::Pending
        }
        Poll::Ready(outcome) => Poll::Ready(outcome.map(|_| ())),
      },
      Some(Work::Sizing(mut future)) => match future.as_mut().poll(cx) {
        Poll::Pending => {
          self.work = Some(Work::Sizing(future));
          Poll::Pending
        }
        Poll::Ready(outcome) => Poll::Ready(outcome.map(|_| ())),
      },
      Some(Work::Opening(mut future)) => match future.as_mut().poll(cx) {
        Poll::Pending => {
          self.work = Some(Work::Opening(future));
          Poll::Pending
        }
        Poll::Ready(Err(failure)) => Poll::Ready(Err(failure)),
        Poll::Ready(Ok(stream)) => {
          self.writer = Some(Writer { stream, cursor: 0 });
          Poll::Ready(Ok(()))
        }
      },
      Some(Work::Pushing(mut future)) => match future.as_mut().poll(cx) {
        Poll::Pending => {
          self.work = Some(Work::Pushing(future));
          Poll::Pending
        }
        Poll::Ready(Err(failure)) => Poll::Ready(Err(failure)),
        Poll::Ready(Ok(writer)) => {
          self.writer = Some(writer);
          Poll::Ready(Ok(()))
        }
      },
      Some(Work::Closing(mut future)) => match future.as_mut().poll(cx) {
        Poll::Pending => {
          self.work = Some(Work::Closing(future));
          Poll::Pending
        }
        Poll::Ready(outcome) => Poll::Ready(outcome),
      },
    }
  }

  /// Gets every held back byte into the open stream.
  fn poll_pushed(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    loop {
      match self.work.take() {
        Some(Work::Opening(mut future)) => match future.as_mut().poll(cx) {
          Poll::Pending => {
            self.work = Some(Work::Opening(future));
            return Poll::Pending;
          }
          Poll::Ready(Err(failure)) => return Poll::Ready(Err(failure)),
          Poll::Ready(Ok(stream)) => {
            self.writer = Some(Writer { stream, cursor: 0 });
          }
        },
        Some(Work::Pushing(mut future)) => match future.as_mut().poll(cx) {
          Poll::Pending => {
            self.work = Some(Work::Pushing(future));
            return Poll::Pending;
          }
          Poll::Ready(Err(failure)) => return Poll::Ready(Err(failure)),
          Poll::Ready(Ok(writer)) => self.writer = Some(writer),
        },
        Some(other) => {
          self.work = Some(other);
          ready!(self.poll_settled(cx))?;
        }
        None => {
          if self.buffer.is_empty() {
            return Poll::Ready(Ok(()));
          }
          match self.writer.take() {
            None => {
              let opening = open_writer(self.handle.clone(), true);
              self.work = Some(Work::Opening(Box::pin(opening)));
            }
            Some(writer) => {
              let bytes = std::mem::take(&mut self.buffer);
              let pushing = push(writer, self.buffer_start, bytes);
              self.work = Some(Work::Pushing(Box::pin(pushing)));
            }
          }
        }
      }
    }
  }

  /// How much of what was read ahead still sits at the cursor.
  fn read_ahead(&self) -> Option<usize> {
    let end = self.read_start + self.read_buffer.len() as u64;
    if self.position < self.read_start || self.position >= end {
      return None;
    }
    Some((end - self.position) as usize)
  }

  /// Lets go of what was read ahead, because the file has moved on from it.
  fn forget_read_ahead(&mut self) {
    self.read_buffer = Vec::new();
    self.read_start = 0;
  }

  /// Moves the cursor to the end of the file as it stands right now.
  ///
  /// The end can only have moved while no stream of ours was open, because
  /// a stream carries its own copy of the file until it closes.
  fn poll_at_end(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    loop {
      match self.work.take() {
        Some(Work::Sizing(mut future)) => match future.as_mut().poll(cx) {
          Poll::Pending => {
            self.work = Some(Work::Sizing(future));
            return Poll::Pending;
          }
          Poll::Ready(Err(failure)) => return Poll::Ready(Err(failure)),
          Poll::Ready(Ok(size)) => {
            self.position = size;
            return Poll::Ready(Ok(()));
          }
        },
        Some(other) => {
          self.work = Some(other);
          ready!(self.poll_settled(cx))?;
        }
        None => {
          let sizing = size_of(self.handle.clone());
          self.work = Some(Work::Sizing(Box::pin(sizing)));
        }
      }
    }
  }

  /// Gets every held back byte all the way into the file.
  fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    loop {
      match self.work.take() {
        Some(Work::Closing(mut future)) => match future.as_mut().poll(cx) {
          Poll::Pending => {
            self.work = Some(Work::Closing(future));
            return Poll::Pending;
          }
          Poll::Ready(outcome) => return Poll::Ready(outcome),
        },
        Some(other) => {
          self.work = Some(other);
          ready!(self.poll_pushed(cx))?;
        }
        None => {
          if !self.buffer.is_empty() {
            ready!(self.poll_pushed(cx))?;
            continue;
          }
          match self.writer.take() {
            None => return Poll::Ready(Ok(())),
            Some(writer) => {
              let closing = close_writer(writer.stream);
              self.work = Some(Work::Closing(Box::pin(closing)));
            }
          }
        }
      }
    }
  }
}

impl AsyncRead for File {
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    let this = self.get_mut();
    if !this.readable {
      return Poll::Ready(Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "the file was not opened for reading",
      )));
    }
    if buf.remaining() == 0 {
      return Poll::Ready(Ok(()));
    }
    loop {
      // Everything that was read ahead is served without going out again.
      if let Some(waiting) = this.read_ahead() {
        let taken = waiting.min(buf.remaining());
        let from = (this.position - this.read_start) as usize;
        buf.put_slice(&this.read_buffer[from..from + taken]);
        this.position += taken as u64;
        return Poll::Ready(Ok(()));
      }
      match this.work.take() {
        Some(Work::Reading(mut future)) => match future.as_mut().poll(cx) {
          Poll::Pending => {
            this.work = Some(Work::Reading(future));
            return Poll::Pending;
          }
          Poll::Ready(Err(failure)) => return Poll::Ready(Err(failure)),
          Poll::Ready(Ok(bytes)) => {
            // Nothing came back, so the cursor is at the end of the file.
            if bytes.is_empty() {
              return Poll::Ready(Ok(()));
            }
            this.read_start = this.position;
            this.read_buffer = bytes;
          }
        },
        Some(other) => {
          this.work = Some(other);
          ready!(this.poll_closed(cx))?;
        }
        None => {
          // A read has to see what was written, and a write reaches the
          // file only when the stream carrying it closes.
          if this.buffer.is_empty() && this.writer.is_none() {
            let wanted = buf.remaining().max(READ_AHEAD);
            let reading =
              read_range(this.handle.clone(), this.position, wanted);
            this.work = Some(Work::Reading(Box::pin(reading)));
          } else {
            ready!(this.poll_closed(cx))?;
          }
        }
      }
    }
  }
}

impl AsyncWrite for File {
  fn poll_write(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    data: &[u8],
  ) -> Poll<io::Result<usize>> {
    let this = self.get_mut();
    if !this.writable {
      return Poll::Ready(Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "the file was not opened for writing",
      )));
    }
    // An appending file writes to the end as it stands, the way `O_APPEND`
    // does. Only the start of a run has to look it up: once bytes are held
    // back, the end is wherever they finish.
    if this.appending && this.buffer.is_empty() && this.writer.is_none() {
      ready!(this.poll_at_end(cx))?;
    }
    // Bytes that do not carry on from the ones already held back cannot
    // join them, and a buffer that has grown far enough goes out on its own.
    let carries_on = this.buffer.is_empty()
      || this.position == this.buffer_start + this.buffer.len() as u64;
    if !carries_on || this.buffer.len() >= WRITE_BUFFER_LIMIT {
      ready!(this.poll_pushed(cx))?;
    }
    if this.buffer.is_empty() {
      this.buffer_start = this.position;
    }
    this.buffer.extend_from_slice(data);
    this.position += data.len() as u64;
    // What was read ahead describes a file that no longer holds.
    this.forget_read_ahead();
    Poll::Ready(Ok(data.len()))
  }

  fn poll_flush(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<()>> {
    self.get_mut().poll_closed(cx)
  }

  fn poll_shutdown(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<()>> {
    self.get_mut().poll_closed(cx)
  }
}

impl AsyncSeek for File {
  fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
    let this = self.get_mut();
    if this.seek.is_some() {
      return Err(io::Error::other("a seek is already under way"));
    }
    this.seek = Some(match position {
      SeekFrom::Start(offset) => Seek::Settled(offset),
      SeekFrom::Current(offset) => {
        Seek::Settled(shifted(this.position, offset)?)
      }
      SeekFrom::End(offset) => Seek::FromEnd(offset),
    });
    Ok(())
  }

  fn poll_complete(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<u64>> {
    let this = self.get_mut();
    loop {
      match this.seek.take() {
        None => return Poll::Ready(Ok(this.position)),
        Some(Seek::Settled(offset)) => {
          this.position = offset;
          return Poll::Ready(Ok(offset));
        }
        Some(Seek::FromEnd(offset)) => {
          this.seek = Some(Seek::FromEnd(offset));
          match this.work.take() {
            Some(Work::Sizing(mut future)) => match future.as_mut().poll(cx) {
              Poll::Pending => {
                this.work = Some(Work::Sizing(future));
                return Poll::Pending;
              }
              Poll::Ready(Err(failure)) => {
                this.seek = None;
                return Poll::Ready(Err(failure));
              }
              Poll::Ready(Ok(size)) => {
                this.seek = Some(Seek::Settled(shifted(size, offset)?));
              }
            },
            Some(other) => {
              this.work = Some(other);
              ready!(this.poll_closed(cx))?;
            }
            None => {
              // The end has to account for held back bytes,
              // so they reach the file before it is measured.
              if this.buffer.is_empty() && this.writer.is_none() {
                let sizing = size_of(this.handle.clone());
                this.work = Some(Work::Sizing(Box::pin(sizing)));
              } else {
                ready!(this.poll_closed(cx))?;
              }
            }
          }
        }
      }
    }
  }
}

impl Drop for File {
  fn drop(&mut self) {
    if self.buffer.is_empty() && self.writer.is_none() {
      return;
    }
    // Nothing can be awaited here, so the last bytes are handed to the
    // JavaScript event loop and left to land on their own.
    let handle = self.handle.clone();
    let start = self.buffer_start;
    let bytes = std::mem::take(&mut self.buffer);
    let writer = self.writer.take();
    spawn_local(async move {
      let landing = async move {
        let mut writer = match writer {
          Some(writer) => writer,
          None => Writer {
            stream: open_writer(handle, true).await?,
            cursor: 0,
          },
        };
        if !bytes.is_empty() {
          writer = push(writer, start, bytes).await?;
        }
        close_writer(writer.stream).await
      };
      if let Err(failure) = landing.await {
        error(&format!(
          "Error `DROPPED_FILE` in `tokio_with_wasm`:\n{failure}"
        ));
      }
    });
  }
}

/// Moves a position, refusing to land before the start of the file.
fn shifted(base: u64, offset: i64) -> io::Result<u64> {
  let landed = if offset >= 0 {
    base.checked_add(offset as u64)
  } else {
    base.checked_sub(offset.unsigned_abs())
  };
  landed.ok_or_else(|| {
    io::Error::new(
      io::ErrorKind::InvalidInput,
      "seek lands outside of the file",
    )
  })
}

/// Reads at most `length` bytes from `start` onwards.
async fn read_range(
  handle: FileSystemFileHandle,
  start: u64,
  length: usize,
) -> io::Result<Vec<u8>> {
  let file =
    cast::<web_sys::File>(await_js(handle.get_file(), Wanted::File).await?)?;
  let size = file.size();
  let start = start as f64;
  if start >= size {
    return Ok(Vec::new());
  }
  let end = size.min(start + length as f64);
  let slice = file
    .slice_with_f64_and_f64(start, end)
    .map_err(|value| super::error::to_io_error(value, Wanted::File))?;
  let buffer = await_js(slice.array_buffer(), Wanted::File).await?;
  Ok(Uint8Array::new(&buffer).to_vec())
}

/// Opens the stream that carries writes into a file.
/// Closing it is what makes those writes count.
async fn open_writer(
  handle: FileSystemFileHandle,
  keep: bool,
) -> io::Result<FileSystemWritableFileStream> {
  let options = FileSystemCreateWritableOptions::new();
  options.set_keep_existing_data(keep);
  cast(
    await_js(handle.create_writable_with_options(&options), Wanted::File)
      .await?,
  )
}

/// Writes bytes into an open stream, moving its cursor along.
async fn push(
  mut writer: Writer,
  start: u64,
  bytes: Vec<u8>,
) -> io::Result<Writer> {
  if writer.cursor != start {
    await_call(writer.stream.seek_with_f64(start as f64), Wanted::File).await?;
    writer.cursor = start;
  }
  await_call(
    writer.stream.write_with_js_u8_array(&unshared(&bytes)),
    Wanted::File,
  )
  .await?;
  writer.cursor += bytes.len() as u64;
  Ok(writer)
}

/// Closes a stream, which is what puts its writes into the file.
async fn close_writer(stream: FileSystemWritableFileStream) -> io::Result<()> {
  await_js(stream.close(), Wanted::File).await?;
  Ok(())
}

/// Reads how long the file is.
async fn size_of(handle: FileSystemFileHandle) -> io::Result<u64> {
  let file =
    cast::<web_sys::File>(await_js(handle.get_file(), Wanted::File).await?)?;
  Ok(file.size() as u64)
}

/// Copies bytes into an array that JavaScript owns.
///
/// A slice of wasm memory reaches JavaScript as a view over that memory,
/// and in the worker build that memory is shared, which the file system
/// API refuses to write from. Copying is what drops the sharing.
fn unshared(bytes: &[u8]) -> Uint8Array {
  let array = Uint8Array::new_with_length(bytes.len() as u32);
  array.copy_from(bytes);
  array
}

/// Reads everything a file holds.
pub(super) async fn read_all(
  handle: &FileSystemFileHandle,
) -> io::Result<Vec<u8>> {
  let file =
    cast::<web_sys::File>(await_js(handle.get_file(), Wanted::File).await?)?;
  let buffer = await_js(file.array_buffer(), Wanted::File).await?;
  Ok(Uint8Array::new(&buffer).to_vec())
}

/// Replaces everything a file holds with `bytes`.
pub(super) async fn overwrite(
  handle: &FileSystemFileHandle,
  bytes: &[u8],
) -> io::Result<()> {
  let stream = open_writer(handle.clone(), false).await?;
  await_call(
    stream.write_with_js_u8_array(&unshared(bytes)),
    Wanted::File,
  )
  .await?;
  close_writer(stream).await
}

/// Empties a file by opening a stream that keeps nothing and closing it.
async fn empty_file(handle: &FileSystemFileHandle) -> io::Result<()> {
  let stream = open_writer(handle.clone(), false).await?;
  close_writer(stream).await
}

/// How a file should be opened, mirroring `tokio::fs::OpenOptions`.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenOptions {
  read: bool,
  write: bool,
  append: bool,
  truncate: bool,
  create: bool,
  create_new: bool,
}

impl OpenOptions {
  /// Starts from a set of options that opens nothing.
  pub fn new() -> OpenOptions {
    OpenOptions::default()
  }

  /// Allows reading.
  pub fn read(&mut self, value: bool) -> &mut OpenOptions {
    self.read = value;
    self
  }

  /// Allows writing.
  pub fn write(&mut self, value: bool) -> &mut OpenOptions {
    self.write = value;
    self
  }

  /// Sends every write to the end of the file rather than to the cursor.
  ///
  /// The end is looked up again whenever a run of writes starts, so an
  /// append that another handle flushed in the meantime is written past
  /// rather than over. `O_APPEND` does that for each single write and does
  /// it atomically; here the two are only as far apart as the last flush.
  pub fn append(&mut self, value: bool) -> &mut OpenOptions {
    self.append = value;
    self
  }

  /// Empties the file when it is opened.
  pub fn truncate(&mut self, value: bool) -> &mut OpenOptions {
    self.truncate = value;
    self
  }

  /// Creates the file if it is missing.
  /// The directories leading to it still have to be there.
  pub fn create(&mut self, value: bool) -> &mut OpenOptions {
    self.create = value;
    self
  }

  /// Creates the file, and fails if it is already there.
  ///
  /// The web API cannot look and create in one step, so this checks first
  /// and creates after. Another tab writing the same name in between wins.
  pub fn create_new(&mut self, value: bool) -> &mut OpenOptions {
    self.create_new = value;
    self
  }

  /// Opens the file that `path` names.
  pub async fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
    let path = path.as_ref();
    if !self.read && !self.write && !self.append {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "a file has to be opened for reading, writing, or appending",
      ));
    }
    if (self.truncate || self.create || self.create_new)
      && !self.write
      && !self.append
    {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "creating or emptying a file needs it opened for writing",
      ));
    }
    if self.truncate && self.append {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "a file cannot be emptied and appended to at once",
      ));
    }

    if self.create_new && file_at(path, false).await.is_ok() {
      return Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "the file is already there",
      ));
    }
    let handle = file_at(path, self.create || self.create_new).await?;
    if self.truncate {
      empty_file(&handle).await?;
    }

    let mut file = File::from_handle(handle);
    file.readable = self.read;
    file.writable = self.write || self.append;
    file.appending = self.append;
    if self.append {
      file.position = size_of(file.handle.clone()).await?;
    }
    Ok(file)
  }
}
