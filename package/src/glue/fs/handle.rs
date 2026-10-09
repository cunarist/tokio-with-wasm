//! Lookups in the origin private file system.

use super::error::{await_js, to_io_error};
use super::path::split_parent;
use js_sys::{Reflect, global};
use std::cell::RefCell;
use std::io;
use std::path::Path;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
  File, FileSystemDirectoryHandle, FileSystemFileHandle,
  FileSystemGetDirectoryOptions, FileSystemGetFileOptions, StorageManager,
};

thread_local! {
  static ROOT: RefCell<Option<FileSystemDirectoryHandle>> =
    const { RefCell::new(None) };
}

async fn root() -> io::Result<FileSystemDirectoryHandle> {
  if let Some(root) = ROOT.with_borrow(Clone::clone) {
    return Ok(root);
  }
  let storage = Reflect::get(&global(), &JsValue::from_str("navigator"))
    .and_then(|navigator| {
      Reflect::get(&navigator, &JsValue::from_str("storage"))
    })
    .map_err(to_io_error)?;
  if storage.is_undefined() {
    return Err(io::Error::new(
      io::ErrorKind::Unsupported,
      "`navigator.storage` is missing; it only exists in a secure context",
    ));
  }
  let root: FileSystemDirectoryHandle =
    await_js(storage.unchecked_into::<StorageManager>().get_directory())
      .await?
      .unchecked_into();
  ROOT.with_borrow_mut(|cached| *cached = Some(root.clone()));
  Ok(root)
}

/// Walks from the root to the directory that `names` leads to.
pub async fn directory_at(
  names: &[String],
  create: bool,
) -> io::Result<FileSystemDirectoryHandle> {
  let mut current = root().await?;
  for name in names {
    current = directory_in(&current, name, create).await?;
  }
  Ok(current)
}

/// Walks to a file. The directories leading to it have to exist already.
pub async fn file_at(
  path: &Path,
  create: bool,
) -> io::Result<FileSystemFileHandle> {
  let (directories, name) = split_parent(path)?;
  file_in(&directory_at(&directories, false).await?, &name, create).await
}

pub async fn file_in(
  parent: &FileSystemDirectoryHandle,
  name: &str,
  create: bool,
) -> io::Result<FileSystemFileHandle> {
  let options = FileSystemGetFileOptions::new();
  options.set_create(create);
  match await_js(parent.get_file_handle_with_options(name, &options)).await {
    Ok(handle) => Ok(handle.unchecked_into()),
    Err(failure) if failure.kind() == io::ErrorKind::NotADirectory => {
      Err(io::Error::new(io::ErrorKind::IsADirectory, failure))
    }
    Err(failure) => Err(failure),
  }
}

pub async fn directory_in(
  parent: &FileSystemDirectoryHandle,
  name: &str,
  create: bool,
) -> io::Result<FileSystemDirectoryHandle> {
  let options = FileSystemGetDirectoryOptions::new();
  options.set_create(create);
  let handle =
    await_js(parent.get_directory_handle_with_options(name, &options)).await?;
  Ok(handle.unchecked_into())
}

/// Takes what the file holds right now.
pub async fn snapshot(handle: &FileSystemFileHandle) -> io::Result<File> {
  Ok(await_js(handle.get_file()).await?.unchecked_into())
}
