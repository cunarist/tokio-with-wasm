//! Asynchronous file system access, backed by the origin private file system.
//!
//! - Paths start at the root of that store; a leading slash is ignored.
//! - `hard_link`, `read_link`, `symlink_metadata`, and `set_permissions`
//!   have no counterpart, so they are missing.
//! - The browser may evict the store unless `navigator.storage.persist()`
//!   was granted.
//! - Writing needs `createWritable` (Safari 26 and later); without it,
//!   writes fail with `ErrorKind::Unsupported`.

mod dir;
mod error;
mod file;
mod handle;
mod path;

pub use dir::{DirEntry, FileType, Metadata, ReadDir};
pub use file::{File, OpenOptions};

use dir::metadata_at;
use error::await_js;
use file::{overwrite, read_range};
use handle::{directory_at, directory_in, file_at, file_in};
use path::{join_names, split_parent, split_path};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use web_sys::FileSystemRemoveOptions;

/// Reads a whole file.
pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
  read_range(file_at(path.as_ref(), false).await?, 0, usize::MAX).await
}

/// Reads a whole file into a string.
pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
  let bytes = read(path).await?;
  String::from_utf8(bytes).map_err(|_| {
    io::Error::new(io::ErrorKind::InvalidData, "file is not UTF-8")
  })
}

/// Writes a whole file, creating it if it is missing.
pub async fn write(
  path: impl AsRef<Path>,
  contents: impl AsRef<[u8]>,
) -> io::Result<()> {
  let handle = file_at(path.as_ref(), true).await?;
  overwrite(&handle, contents.as_ref()).await
}

/// Copies a file, returning how many bytes it holds.
pub async fn copy(
  from: impl AsRef<Path>,
  to: impl AsRef<Path>,
) -> io::Result<u64> {
  let bytes = read(from).await?;
  write(to, &bytes).await?;
  Ok(bytes.len() as u64)
}

/// Moves a file or a directory.
///
/// The web has no move, so this copies and then removes: a failure halfway
/// leaves both behind, and the cost grows with what is moved.
pub async fn rename(
  from: impl AsRef<Path>,
  to: impl AsRef<Path>,
) -> io::Result<()> {
  let from = from.as_ref();
  let to = to.as_ref();
  let is_dir = metadata_at(from).await?.is_dir();
  let (from_names, to_names) = (split_path(from)?, split_path(to)?);
  if from_names == to_names {
    return Ok(());
  }
  if !is_dir {
    copy(from, to).await?;
    return remove_file(from).await;
  }
  if to_names.starts_with(&from_names) {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "a directory cannot be moved into itself",
    ));
  }
  // Only an empty directory may be replaced.
  match remove_dir(to).await {
    Err(failure) if failure.kind() == io::ErrorKind::DirectoryNotEmpty => {
      return Err(failure);
    }
    _ => {}
  }
  copy_directory(from.to_owned(), to.to_owned()).await?;
  remove_dir_all(from).await
}

fn copy_directory(
  from: PathBuf,
  to: PathBuf,
) -> Pin<Box<dyn Future<Output = io::Result<()>>>> {
  Box::pin(async move {
    create_dir_all(&to).await?;
    let mut entries = read_dir(&from).await?;
    while let Some(entry) = entries.next_entry().await? {
      let target = to.join(entry.file_name());
      if entry.file_type().await?.is_dir() {
        copy_directory(entry.path(), target).await?;
      } else {
        copy(entry.path(), target).await?;
      }
    }
    Ok(())
  })
}

/// Creates a directory. The directories leading to it have to be there.
pub async fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
  let path = path.as_ref();
  // The web API would hand back an existing directory instead of failing.
  if try_exists(path).await? {
    return Err(io::Error::new(
      io::ErrorKind::AlreadyExists,
      "the name is already taken",
    ));
  }
  let (directories, name) = split_parent(path)?;
  directory_in(&directory_at(&directories, false).await?, &name, true).await?;
  Ok(())
}

/// Creates a directory and every directory leading to it.
pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
  directory_at(&split_path(path.as_ref())?, true).await?;
  Ok(())
}

/// Removes an empty directory.
pub async fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
  remove(path.as_ref(), true, false).await
}

/// Removes a directory and everything under it.
pub async fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
  remove(path.as_ref(), true, true).await
}

/// Removes a file.
pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
  remove(path.as_ref(), false, false).await
}

async fn remove(path: &Path, dir: bool, recursive: bool) -> io::Result<()> {
  let (directories, name) = split_parent(path)?;
  let parent = directory_at(&directories, false).await?;
  // The web API removes either kind, so the kind is checked first.
  if dir {
    directory_in(&parent, &name, false).await?;
  } else {
    file_in(&parent, &name, false).await?;
  }
  let options = FileSystemRemoveOptions::new();
  options.set_recursive(recursive);
  await_js(parent.remove_entry_with_options(&name, &options)).await?;
  Ok(())
}

/// Reports what is known about the entry that `path` names.
pub async fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
  metadata_at(path.as_ref()).await
}

/// Returns whether anything lives at `path`.
pub async fn try_exists(path: impl AsRef<Path>) -> io::Result<bool> {
  match metadata_at(path.as_ref()).await {
    Ok(_) => Ok(true),
    Err(failure) if failure.kind() == io::ErrorKind::NotFound => Ok(false),
    Err(failure) => Err(failure),
  }
}

/// Lists what a directory holds.
pub async fn read_dir(path: impl AsRef<Path>) -> io::Result<ReadDir> {
  let names = split_path(path.as_ref())?;
  let handle = directory_at(&names, false).await?;
  Ok(ReadDir::new(&handle, join_names(&names)))
}

/// Spells a path the one way that names its entry, which has to exist.
pub async fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
  let spelled = join_names(&split_path(path.as_ref())?);
  metadata_at(&spelled).await?;
  Ok(spelled)
}
