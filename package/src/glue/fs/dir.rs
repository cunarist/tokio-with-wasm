//! Reading directories, and what the entries in them say about themselves.

use super::error::to_io_error;
use super::handle::{directory_at, directory_in, file_in, snapshot};
use super::path::split_path;
use js_sys::{AsyncIterator, IteratorNext};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
  FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemHandle,
  FileSystemHandleKind,
};

/// What an entry in the origin private file system is.
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

  /// Always false.
  pub fn is_symlink(&self) -> bool {
    false
  }
}

/// What is known about an entry. There is no `permissions` on the web.
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

  /// Always false.
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

  /// Returns when the file was last written to. Directories have no time.
  pub fn modified(&self) -> io::Result<SystemTime> {
    self.modified.ok_or_else(|| {
      io::Error::new(
        io::ErrorKind::Unsupported,
        "the origin private file system keeps no timestamp for a directory",
      )
    })
  }
}

pub async fn file_metadata(
  handle: &FileSystemFileHandle,
) -> io::Result<Metadata> {
  let file = snapshot(handle).await?;
  Ok(Metadata {
    file_type: FileType {
      is_directory: false,
    },
    length: file.size() as u64,
    modified: Some(
      UNIX_EPOCH + Duration::from_millis(file.last_modified() as u64),
    ),
  })
}

const DIRECTORY: Metadata = Metadata {
  file_type: FileType { is_directory: true },
  length: 0,
  modified: None,
};

/// A stream over the entries of a directory,
/// returned by [`read_dir`](super::read_dir).
pub struct ReadDir {
  entries: AsyncIterator,
  /// Kept across a cancelled call, so no entry is lost.
  step: Option<JsFuture>,
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
      step: None,
      directory,
    }
  }

  /// Returns the next entry, or `None` once the directory is exhausted.
  pub async fn next_entry(&mut self) -> io::Result<Option<DirEntry>> {
    let step = match self.step.take() {
      Some(step) => step,
      None => self.entries.next().map_err(to_io_error)?.into(),
    };
    let step = self.step.insert(step).await;
    self.step = None;
    let step: IteratorNext = step.map_err(to_io_error)?.unchecked_into();
    Ok((!step.done()).then(|| DirEntry {
      handle: step.value().unchecked_into(),
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
      return Ok(DIRECTORY);
    }
    file_metadata(self.handle.unchecked_ref()).await
  }
}

/// Reports what lives at `path`, whichever kind of entry that is.
pub async fn metadata_at(path: &Path) -> io::Result<Metadata> {
  let names = split_path(path)?;
  let Some((name, directories)) = names.split_last() else {
    return Ok(DIRECTORY);
  };
  let parent = directory_at(directories, false).await?;
  // Asking for both kinds avoids relying on how browsers name the failure.
  match file_in(&parent, name, false).await {
    Ok(handle) => file_metadata(&handle).await,
    Err(failure) if failure.kind() == io::ErrorKind::NotFound => Err(failure),
    Err(_) => directory_in(&parent, name, false).await.map(|_| DIRECTORY),
  }
}
