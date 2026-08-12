//! Lookups in the origin private file system.

use super::error::{Wanted, await_js, cast, to_io_error};
use super::path::split_parent;
use js_sys::{Reflect, global};
use std::cell::RefCell;
use std::io;
use std::path::Path;
use wasm_bindgen::JsValue;
use web_sys::{
  FileSystemDirectoryHandle, FileSystemFileHandle,
  FileSystemGetDirectoryOptions, FileSystemGetFileOptions, StorageManager,
};

thread_local! {
  /// The root of the origin private file system.
  /// Every lookup starts here, so it is fetched once per thread.
  static ROOT: RefCell<Option<FileSystemDirectoryHandle>> =
    const { RefCell::new(None) };
}

/// Returns the root directory of the origin private file system.
pub async fn root() -> io::Result<FileSystemDirectoryHandle> {
  if let Some(root) = ROOT.with_borrow(Clone::clone) {
    return Ok(root);
  }
  let storage = Reflect::get(&global(), &JsValue::from_str("navigator"))
    .and_then(|navigator| {
      Reflect::get(&navigator, &JsValue::from_str("storage"))
    })
    .map_err(|value| to_io_error(value, Wanted::Either))?;
  if storage.is_undefined() || storage.is_null() {
    return Err(io::Error::new(
      io::ErrorKind::Unsupported,
      "`navigator.storage` is missing, so there is no file system to reach; \
       it is only handed out in a secure context",
    ));
  }
  let storage = cast::<StorageManager>(storage)?;
  let root = cast::<FileSystemDirectoryHandle>(
    await_js(storage.get_directory(), Wanted::Directory).await?,
  )?;
  ROOT.with_borrow_mut(|cached| *cached = Some(root.clone()));
  Ok(root)
}

/// Walks from the root to the directory that `names` leads to,
/// creating the directories along the way when asked to.
pub async fn directory_at(
  names: &[String],
  create: bool,
) -> io::Result<FileSystemDirectoryHandle> {
  let options = FileSystemGetDirectoryOptions::new();
  options.set_create(create);
  let mut current = root().await?;
  for name in names {
    let next = await_js(
      current.get_directory_handle_with_options(name, &options),
      Wanted::Directory,
    )
    .await?;
    current = cast(next)?;
  }
  Ok(current)
}

/// Walks to the file that `path` names.
/// The directories leading to it have to exist already,
/// which is how `tokio` behaves too.
pub async fn file_at(
  path: &Path,
  create: bool,
) -> io::Result<FileSystemFileHandle> {
  let (directories, name) = split_parent(path)?;
  let parent = directory_at(&directories, false).await?;
  file_in(&parent, &name, create).await
}

/// Looks a file up inside a directory that has already been walked to.
pub async fn file_in(
  parent: &FileSystemDirectoryHandle,
  name: &str,
  create: bool,
) -> io::Result<FileSystemFileHandle> {
  let options = FileSystemGetFileOptions::new();
  options.set_create(create);
  let handle = await_js(
    parent.get_file_handle_with_options(name, &options),
    Wanted::File,
  )
  .await?;
  cast(handle)
}

/// Looks a directory up inside a directory that has already been walked to.
pub async fn directory_in(
  parent: &FileSystemDirectoryHandle,
  name: &str,
  create: bool,
) -> io::Result<FileSystemDirectoryHandle> {
  let options = FileSystemGetDirectoryOptions::new();
  options.set_create(create);
  let handle = await_js(
    parent.get_directory_handle_with_options(name, &options),
    Wanted::Directory,
  )
  .await?;
  cast(handle)
}

/// Whichever kind of entry a name turned out to be.
pub enum Entry {
  File(FileSystemFileHandle),
  Directory,
}

/// Looks a name up without knowing which kind it is.
///
/// Both kinds are asked for and the one that answers wins, so nothing is
/// read out of which exception came back. Browsers agree on what a
/// successful lookup means far more than on what they name a failure.
pub async fn entry_in(
  parent: &FileSystemDirectoryHandle,
  name: &str,
) -> io::Result<Entry> {
  match file_in(parent, name, false).await {
    Ok(handle) => return Ok(Entry::File(handle)),
    // Only a missing entry rules out the other kind as well.
    Err(failure) if failure.kind() == io::ErrorKind::NotFound => {
      return Err(failure);
    }
    Err(_) => {}
  }
  directory_in(parent, name, false).await?;
  Ok(Entry::Directory)
}
