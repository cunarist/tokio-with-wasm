//! Files in the origin private file system.

use super::dir::{Metadata, file_metadata, metadata_at};
use super::error::{await_call, await_js, to_io_error};
use super::handle::{file_at, snapshot};
use crate::error;
use js_sys::{Reflect, Uint8Array};
use std::future::{Future, poll_fn};
use std::io::{self, SeekFrom};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
  FileSystemCreateWritableOptions, FileSystemFileHandle,
  FileSystemWritableFileStream,
};

/// How many written bytes are held in memory before going to the stream.
const WRITE_BUFFER_LIMIT: usize = 1 << 20;
/// Every read is a round trip through JavaScript, so small reads share one.
const READ_AHEAD: usize = 256 * 1024;

type Started<T> = Pin<Box<dyn Future<Output = io::Result<T>>>>;

/// The open stream that carries writes, and where its cursor sits.
struct Writer {
  stream: FileSystemWritableFileStream,
  cursor: u64,
}

/// The one call a file can have in flight.
enum Work {
  Reading(Started<Vec<u8>>),
  Pushing(Started<Writer>),
  Closing(Started<JsValue>),
  Sizing(Started<u64>),
}

/// An open file in the origin private file system.
///
/// Writes land in the file only on flush, shutdown, or drop, because the web
/// API commits them when the stream carrying them closes. Of two files whose
/// writes overlap in time, the last to close wins, but a crash never leaves
/// a half-written file.
pub struct File {
  handle: FileSystemFileHandle,
  position: u64,
  /// Written bytes not yet pushed to the stream, which start at `buffer_start`.
  buffer: Vec<u8>,
  buffer_start: u64,
  /// Bytes read ahead, which start at `read_start`.
  read_buffer: Vec<u8>,
  read_start: u64,
  writer: Option<Writer>,
  work: Option<Work>,
  seek: Option<SeekFrom>,
  readable: bool,
  writable: bool,
  appending: bool,
}

impl File {
  /// Opens a file for reading.
  pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
    OpenOptions::new().read(true).open(path).await
  }

  /// Opens a file for writing, creating or emptying it.
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

  /// Reports what is known about the file. Unflushed bytes do not count.
  pub async fn metadata(&self) -> io::Result<Metadata> {
    file_metadata(&self.handle).await
  }

  /// Grows or shrinks the file to `size` bytes.
  pub async fn set_len(&mut self, size: u64) -> io::Result<()> {
    self.settle().await?;
    self.read_buffer = Vec::new();
    let stream = open_writer(&self.handle, true).await?;
    await_call(stream.truncate_with_f64(size as f64)).await?;
    await_js(stream.close()).await?;
    Ok(())
  }

  /// Writes every held back byte into the file.
  pub async fn sync_all(&mut self) -> io::Result<()> {
    self.settle().await
  }

  /// Same as [`File::sync_all`], as the web keeps no separate metadata.
  pub async fn sync_data(&mut self) -> io::Result<()> {
    self.settle().await
  }

  async fn settle(&mut self) -> io::Result<()> {
    poll_fn(|cx| self.poll_flushed(cx, true)).await
  }

  /// Finishes the call in flight and keeps what it hands back.
  fn poll_work(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    let done = match &mut self.work {
      None => return Poll::Ready(Ok(())),
      Some(Work::Reading(future)) => {
        ready!(future.as_mut().poll(cx)).map(|bytes| {
          self.read_start = self.position;
          self.read_buffer = bytes;
        })
      }
      Some(Work::Pushing(future)) => ready!(future.as_mut().poll(cx))
        .map(|writer| self.writer = Some(writer)),
      Some(Work::Closing(future)) => ready!(future.as_mut().poll(cx)).map(drop),
      Some(Work::Sizing(future)) => ready!(future.as_mut().poll(cx)).map(drop),
    };
    self.work = None;
    Poll::Ready(done)
  }

  /// Pushes held back bytes into the stream, and closes it if `close`.
  fn poll_flushed(
    &mut self,
    cx: &mut Context<'_>,
    close: bool,
  ) -> Poll<io::Result<()>> {
    loop {
      ready!(self.poll_work(cx))?;
      self.work = Some(if !self.buffer.is_empty() {
        let bytes = std::mem::take(&mut self.buffer);
        let writer = self.writer.take();
        let pushing =
          push(self.handle.clone(), writer, self.buffer_start, bytes);
        Work::Pushing(Box::pin(pushing))
      } else if let Some(writer) = self.writer.take_if(|_| close) {
        Work::Closing(Box::pin(await_js(writer.stream.close())))
      } else {
        return Poll::Ready(Ok(()));
      });
    }
  }

  /// Measures the file, once every held back byte is in it.
  fn poll_size(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
    loop {
      if let Some(Work::Sizing(future)) = &mut self.work {
        let size = ready!(future.as_mut().poll(cx));
        self.work = None;
        return Poll::Ready(size);
      }
      ready!(self.poll_flushed(cx, true))?;
      let handle = self.handle.clone();
      self.work = Some(Work::Sizing(Box::pin(async move {
        Ok(snapshot(&handle).await?.size() as u64)
      })));
    }
  }

  /// Moves the cursor to where an appended write lands.
  fn poll_at_end(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    if !self.buffer.is_empty() {
      self.position = self.buffer_start + self.buffer.len() as u64;
      return Poll::Ready(Ok(()));
    }
    // A push in flight holds our open stream.
    if let Some(Work::Pushing(_)) = self.work {
      ready!(self.poll_work(cx))?;
    }
    self.position = match &self.writer {
      Some(writer) => writer.cursor,
      None => ready!(self.poll_size(cx))?,
    };
    Poll::Ready(Ok(()))
  }

  /// Drops a read in flight, which was meant for another cursor or content.
  fn forget_reading(&mut self) {
    if let Some(Work::Reading(_)) = self.work {
      self.work = None;
    }
  }
}

impl AsyncRead for File {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    if !self.readable {
      return Poll::Ready(Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "the file was not opened for reading",
      )));
    }
    // Like `tokio`, finish a pending seek first and ignore its failure.
    let _ = ready!(self.as_mut().poll_complete(cx));
    let this = self.get_mut();
    if buf.remaining() == 0 {
      return Poll::Ready(Ok(()));
    }
    loop {
      let end = this.read_start + this.read_buffer.len() as u64;
      if (this.read_start..end).contains(&this.position) {
        let ahead =
          &this.read_buffer[(this.position - this.read_start) as usize..];
        let taken = ahead.len().min(buf.remaining());
        buf.put_slice(&ahead[..taken]);
        this.position += taken as u64;
        return Poll::Ready(Ok(()));
      }
      if let Some(Work::Reading(_)) = this.work {
        ready!(this.poll_work(cx))?;
        // Nothing came back, so the cursor is at the end of the file.
        if this.read_buffer.is_empty() {
          return Poll::Ready(Ok(()));
        }
        continue;
      }
      // A read only sees writes once the stream carrying them closes.
      ready!(this.poll_flushed(cx, true))?;
      let wanted = buf.remaining().max(READ_AHEAD);
      let reading = read_range(this.handle.clone(), this.position, wanted);
      this.work = Some(Work::Reading(Box::pin(reading)));
    }
  }
}

impl AsyncWrite for File {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    data: &[u8],
  ) -> Poll<io::Result<usize>> {
    if !self.writable {
      return Poll::Ready(Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "the file was not opened for writing",
      )));
    }
    let _ = ready!(self.as_mut().poll_complete(cx));
    let this = self.get_mut();
    // Like `O_APPEND`.
    if this.appending {
      ready!(this.poll_at_end(cx))?;
    }
    let end = this.buffer_start + this.buffer.len() as u64;
    if this.position != end || this.buffer.len() >= WRITE_BUFFER_LIMIT {
      ready!(this.poll_flushed(cx, false))?;
    }
    if this.buffer.is_empty() {
      this.buffer_start = this.position;
    }
    this.buffer.extend_from_slice(data);
    this.position += data.len() as u64;
    this.read_buffer = Vec::new();
    this.forget_reading();
    Poll::Ready(Ok(data.len()))
  }

  fn poll_flush(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<()>> {
    self.get_mut().poll_flushed(cx, true)
  }

  fn poll_shutdown(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<()>> {
    self.get_mut().poll_flushed(cx, true)
  }
}

impl AsyncSeek for File {
  fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
    let this = self.get_mut();
    if this.seek.is_some() {
      return Err(io::Error::other("a seek is already under way"));
    }
    this.seek = Some(position);
    Ok(())
  }

  fn poll_complete(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<u64>> {
    let this = self.get_mut();
    let landed = match this.seek {
      None => return Poll::Ready(Ok(this.position)),
      Some(SeekFrom::Start(offset)) => Ok(offset),
      Some(SeekFrom::Current(offset)) => shifted(this.position, offset),
      Some(SeekFrom::End(offset)) => {
        ready!(this.poll_size(cx)).and_then(|size| shifted(size, offset))
      }
    };
    this.seek = None;
    let landed = landed?;
    if landed != this.position {
      this.forget_reading();
      this.position = landed;
    }
    Poll::Ready(Ok(landed))
  }
}

impl Drop for File {
  fn drop(&mut self) {
    let pushing = matches!(self.work, Some(Work::Pushing(_)));
    if self.buffer.is_empty() && self.writer.is_none() && !pushing {
      return;
    }
    // Nothing can be awaited here, so the rest lands on the event loop.
    let mut rest =
      std::mem::replace(self, File::from_handle(self.handle.clone()));
    spawn_local(async move {
      if let Err(failure) = rest.settle().await {
        error(&format!(
          "Error `DROPPED_FILE` in `tokio_with_wasm`:\n{failure}"
        ));
      }
    });
  }
}

fn shifted(base: u64, offset: i64) -> io::Result<u64> {
  base.checked_add_signed(offset).ok_or_else(|| {
    io::Error::new(
      io::ErrorKind::InvalidInput,
      "seek lands outside of the file",
    )
  })
}

/// Reads at most `length` bytes from `start` onwards.
pub(super) async fn read_range(
  handle: FileSystemFileHandle,
  start: u64,
  length: usize,
) -> io::Result<Vec<u8>> {
  // `slice` clamps both ends to the size.
  let start = start as f64;
  let slice = snapshot(&handle)
    .await?
    .slice_with_f64_and_f64(start, start + length as f64)
    .map_err(to_io_error)?;
  let buffer = await_js(slice.array_buffer()).await?;
  Ok(Uint8Array::new(&buffer).to_vec())
}

/// Opens the stream that carries writes; closing it commits them.
async fn open_writer(
  handle: &FileSystemFileHandle,
  keep: bool,
) -> io::Result<FileSystemWritableFileStream> {
  if !Reflect::has(handle, &"createWritable".into()).unwrap_or(false) {
    return Err(io::Error::new(
      io::ErrorKind::Unsupported,
      "this browser cannot write to files",
    ));
  }
  let options = FileSystemCreateWritableOptions::new();
  options.set_keep_existing_data(keep);
  let stream = await_js(handle.create_writable_with_options(&options)).await?;
  Ok(stream.unchecked_into())
}

/// Writes bytes at `start`, opening a stream first if none is open.
async fn push(
  handle: FileSystemFileHandle,
  writer: Option<Writer>,
  start: u64,
  bytes: Vec<u8>,
) -> io::Result<Writer> {
  let mut writer = match writer {
    Some(writer) => writer,
    None => Writer {
      stream: open_writer(&handle, true).await?,
      cursor: 0,
    },
  };
  if writer.cursor != start {
    await_call(writer.stream.seek_with_f64(start as f64)).await?;
  }
  // A copy, as the API refuses views of shared wasm memory.
  let array = Uint8Array::from(&bytes[..]);
  await_call(writer.stream.write_with_js_u8_array(&array)).await?;
  writer.cursor = start + bytes.len() as u64;
  Ok(writer)
}

/// Replaces everything a file holds with `bytes`.
pub(super) async fn overwrite(
  handle: &FileSystemFileHandle,
  bytes: &[u8],
) -> io::Result<()> {
  let stream = open_writer(handle, false).await?;
  await_call(stream.write_with_js_u8_array(&Uint8Array::from(bytes))).await?;
  await_js(stream.close()).await?;
  Ok(())
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
  /// Unlike `O_APPEND`, the end is looked up only when no stream is open.
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
  /// This checks and creates in two steps, which is not atomic.
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

    if self.create_new && metadata_at(path).await.is_ok() {
      return Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "the file is already there",
      ));
    }
    let handle = file_at(path, self.create || self.create_new).await?;
    if self.truncate {
      overwrite(&handle, &[]).await?;
    }

    let mut file = File::from_handle(handle);
    file.readable = self.read;
    file.writable = self.write || self.append;
    file.appending = self.append;
    Ok(file)
  }
}
