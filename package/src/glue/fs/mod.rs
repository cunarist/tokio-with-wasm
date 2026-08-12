//! Asynchronous file system access.
//!
//! Files live in the origin private file system, a store that the browser
//! keeps on disk for one origin and hands to no one else. It is reached
//! without asking the user for anything, and every engine has it, which is
//! why it backs this module rather than the file pickers do: those only
//! exist in desktop Chromium, and only inside a click.
//!
//! What follows from that store rather than from `tokio`:
//!
//! - Paths start at the root of the store, not at the root of a disk. A
//!   leading slash is accepted and ignored, and `..` is resolved before a
//!   path is used, which is safe because there are no symbolic links.
//! - `hard_link`, `read_link`, `symlink_metadata`, and `set_permissions`
//!   have no counterpart here, so they are missing rather than failing.
//! - The store shares one quota with the other storage APIs, and the
//!   browser may throw all of it away under disk pressure unless
//!   `navigator.storage.persist()` has been granted.
//! - Nothing outside the page can see these files. Handing one to the user
//!   means the page has to offer it as a download.

mod dir;
mod error;
mod file;
mod handle;
mod path;

pub use dir::{DirEntry, FileType, Metadata, ReadDir};
pub use file::{File, OpenOptions};

use dir::metadata_at;
use error::{Wanted, await_js};
use file::{overwrite, read_all};
use handle::{directory_at, directory_in, file_at, file_in};
use path::{join_names, split_parent, split_path};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use web_sys::FileSystemRemoveOptions;

/// Reads a whole file.
pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
  let handle = file_at(path.as_ref(), false).await?;
  read_all(&handle).await
}

/// Reads a whole file into a string.
pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
  let bytes = read(path).await?;
  String::from_utf8(bytes).map_err(|_| {
    io::Error::new(io::ErrorKind::InvalidData, "file is not UTF-8")
  })
}

/// Writes to a file, creating it if it is missing
/// and replacing everything in it if it is not.
///
/// The directories leading to the file have to be there already.
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
  let source = file_at(from.as_ref(), false).await?;
  let bytes = read_all(&source).await?;
  let length = bytes.len() as u64;
  let destination = file_at(to.as_ref(), true).await?;
  overwrite(&destination, &bytes).await?;
  Ok(length)
}

/// Moves a file or a directory to another name.
///
/// The web API has no move, so this copies and then removes. That is not
/// one step: a failure halfway leaves both ends behind, and the cost grows
/// with what is being moved rather than staying flat.
pub async fn rename(
  from: impl AsRef<Path>,
  to: impl AsRef<Path>,
) -> io::Result<()> {
  let from = from.as_ref();
  let to = to.as_ref();
  if metadata_at(from).await?.is_dir() {
    copy_directory(from.to_owned(), to.to_owned()).await?;
    remove_dir_all(from).await
  } else {
    copy(from, to).await?;
    remove_file(from).await
  }
}

/// Copies a directory and everything under it.
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
  let (directories, name) = split_parent(path.as_ref())?;
  let parent = directory_at(&directories, false).await?;
  // The web API hands back a directory that is already there rather than
  // refusing, so looking for one comes first.
  let taken = match directory_in(&parent, &name, false).await {
    Ok(_) => true,
    // A file is sitting on the name, which `tokio` also calls taken.
    Err(failure) if failure.kind() == io::ErrorKind::NotADirectory => true,
    Err(failure) if failure.kind() == io::ErrorKind::NotFound => false,
    Err(failure) => return Err(failure),
  };
  if taken {
    return Err(io::Error::new(
      io::ErrorKind::AlreadyExists,
      "the name is already taken",
    ));
  }
  directory_in(&parent, &name, true).await?;
  Ok(())
}

/// Creates a directory and every directory leading to it.
pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
  let names = split_path(path.as_ref())?;
  directory_at(&names, true).await?;
  Ok(())
}

/// Removes an empty directory.
pub async fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
  let (directories, name) = split_parent(path.as_ref())?;
  let parent = directory_at(&directories, false).await?;
  // Asking for it as a directory is what rules out a file by that name.
  directory_in(&parent, &name, false).await?;
  // Left alone, the web API refuses a directory that still holds something.
  await_js(parent.remove_entry(&name), Wanted::Directory).await?;
  Ok(())
}

/// Removes a directory and everything under it.
pub async fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
  let (directories, name) = split_parent(path.as_ref())?;
  let parent = directory_at(&directories, false).await?;
  directory_in(&parent, &name, false).await?;
  let options = FileSystemRemoveOptions::new();
  options.set_recursive(true);
  await_js(
    parent.remove_entry_with_options(&name, &options),
    Wanted::Directory,
  )
  .await?;
  Ok(())
}

/// Removes a file.
pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
  let (directories, name) = split_parent(path.as_ref())?;
  let parent = directory_at(&directories, false).await?;
  // Asking for it as a file is what rules out a directory by that name.
  file_in(&parent, &name, false).await?;
  await_js(parent.remove_entry(&name), Wanted::File).await?;
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

/// Spells a path the one way that names the entry it leads to.
///
/// Nothing has to be followed to do this, because the origin private file
/// system has no symbolic links. The entry still has to be there, the same
/// way it does in `tokio`.
pub async fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
  let names = split_path(path.as_ref())?;
  let spelled = join_names(&names);
  metadata_at(&spelled).await?;
  Ok(spelled)
}
