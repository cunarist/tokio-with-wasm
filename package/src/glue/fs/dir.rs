//! Reading directories, and what the entries in them say about themselves.

use super::error::{Wanted, await_js, cast, to_io_error};
use super::handle::{Entry, directory_at, entry_in};
use super::path::split_path;
use js_sys::{AsyncIterator, Reflect};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasm_bindgen::JsValue;
use web_sys::{
  FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemHandle,
  FileSystemHandleKind,
};

/// What an entry in the origin private file system is.
///
/// There are no symbolic links to be had, so `is_symlink` is always false.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileType {
  is_directory: bool,
}

impl FileType {
  /// Returns true when the entry is a file.
  pub fn is_file(&self) -> bool {
    !self.is_directory
  }

  /// Returns true when the entry is a directory.
  pub fn is_dir(&self) -> bool {
    self.is_directory
  }

  /// Always returns false,
  /// because the origin private file system has no symbolic links.
  pub fn is_symlink(&self) -> bool {
    false
  }
}

/// What is known about an entry in the origin private file system.
///
/// `tokio` also hands out permissions here. There are none to hand out on
/// the web, where every entry belongs to the origin that wrote it, so
/// `permissions` is missing rather than lying about what it returns.
#[derive(Clone, Copy, Debug)]
pub struct Metadata {
  file_type: FileType,
  length: u64,
  modified: Option<SystemTime>,
}

impl Metadata {
  /// Returns what the entry is.
  pub fn file_type(&self) -> FileType {
    self.file_type
  }

  /// Returns true when the entry is a file.
  pub fn is_file(&self) -> bool {
    self.file_type.is_file()
  }

  /// Returns true when the entry is a directory.
  pub fn is_dir(&self) -> bool {
    self.file_type.is_dir()
  }

  /// Always returns false,
  /// because the origin private file system has no symbolic links.
  pub fn is_symlink(&self) -> bool {
    false
  }

  /// Returns the size of the file in bytes, or zero for a directory.
  pub fn len(&self) -> u64 {
    self.length
  }

  /// Returns true when the file holds nothing.
  pub fn is_empty(&self) -> bool {
    self.length == 0
  }

  /// Returns when the file was last written to.
  ///
  /// Directories carry no timestamp of their own,
  /// so asking one is an error rather than a made up time.
  pub fn modified(&self) -> io::Result<SystemTime> {
    self.modified.ok_or_else(|| {
      io::Error::new(
        io::ErrorKind::Unsupported,
        "the origin private file system keeps no timestamp for a directory",
      )
    })
  }
}

/// Reads the size and timestamp that a file handle points at.
pub async fn file_metadata(
  handle: &FileSystemFileHandle,
) -> io::Result<Metadata> {
  let file =
    cast::<web_sys::File>(await_js(handle.get_file(), Wanted::File).await?)?;
  Ok(Metadata {
    file_type: FileType {
      is_directory: false,
    },
    length: file.size() as u64,
    // `lastModified` counts milliseconds from the same epoch that
    // `SystemTime` does, so no clock has to be read to convert it.
    modified: Some(
      UNIX_EPOCH + Duration::from_millis(file.last_modified() as u64),
    ),
  })
}

/// Describes a directory, which has nothing to report but its kind.
pub fn directory_metadata() -> Metadata {
  Metadata {
    file_type: FileType { is_directory: true },
    length: 0,
    modified: None,
  }
}

/// A stream over the entries of a directory,
/// returned by [`read_dir`](super::read_dir).
pub struct ReadDir {
  entries: AsyncIterator,
  directory: PathBuf,
}

impl ReadDir {
  /// Wraps the entries of a directory that lives at `directory`.
  pub(super) fn new(
    handle: &FileSystemDirectoryHandle,
    directory: PathBuf,
  ) -> Self {
    ReadDir {
      entries: handle.values(),
      directory,
    }
  }

  /// Returns the next entry, or `None` once the directory is exhausted.
  ///
  /// The directory is walked as it is asked for, so an entry written
  /// after the walk started may or may not show up, the same way it may
  /// or may not in `tokio`.
  pub async fn next_entry(&mut self) -> io::Result<Option<DirEntry>> {
    let step = self
      .entries
      .next()
      .map_err(|value| to_io_error(value, Wanted::Either))?;
    let step = await_js(step, Wanted::Either).await?;
    let done = Reflect::get(&step, &JsValue::from_str("done"))
      .map_err(|value| to_io_error(value, Wanted::Either))?
      .is_truthy();
    if done {
      return Ok(None);
    }
    let value = Reflect::get(&step, &JsValue::from_str("value"))
      .map_err(|value| to_io_error(value, Wanted::Either))?;
    Ok(Some(DirEntry {
      handle: cast::<FileSystemHandle>(value)?,
      directory: self.directory.clone(),
    }))
  }
}

/// One entry of a directory, handed out by [`ReadDir::next_entry`].
pub struct DirEntry {
  handle: FileSystemHandle,
  directory: PathBuf,
}

impl DirEntry {
  /// Returns the name of the entry, without the directories leading to it.
  pub fn file_name(&self) -> OsString {
    OsString::from(self.handle.name())
  }

  /// Returns the path that leads to the entry from the root.
  pub fn path(&self) -> PathBuf {
    self.directory.join(self.handle.name())
  }

  /// Returns what the entry is.
  pub async fn file_type(&self) -> io::Result<FileType> {
    Ok(FileType {
      is_directory: self.handle.kind() == FileSystemHandleKind::Directory,
    })
  }

  /// Returns what is known about the entry.
  pub async fn metadata(&self) -> io::Result<Metadata> {
    if self.handle.kind() == FileSystemHandleKind::Directory {
      return Ok(directory_metadata());
    }
    let handle = cast::<FileSystemFileHandle>(JsValue::from(&self.handle))?;
    file_metadata(&handle).await
  }
}

/// Reports what lives at `path`, whichever kind of entry that is.
pub async fn metadata_at(path: &Path) -> io::Result<Metadata> {
  let names = split_path(path)?;
  // The root is a directory that no lookup returns, so answer for it here.
  let Some((name, directories)) = names.split_last() else {
    return Ok(directory_metadata());
  };
  let parent = directory_at(directories, false).await?;
  match entry_in(&parent, name).await? {
    Entry::File(handle) => file_metadata(&handle).await,
    Entry::Directory => Ok(directory_metadata()),
  }
}
