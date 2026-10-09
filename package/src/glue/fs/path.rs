//! Path handling for the origin private file system.

use std::io;
use std::path::{Component, Path, PathBuf};

/// Splits a path into the names that lead from the root to an entry.
///
/// `..` is resolved here, which is safe because there are no symbolic links.
pub fn split_path(path: &Path) -> io::Result<Vec<String>> {
  let mut names = Vec::new();
  for component in path.components() {
    match component {
      Component::RootDir | Component::CurDir => continue,
      Component::ParentDir => {
        if names.pop().is_none() {
          return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path leads out of the origin private file system",
          ));
        }
      }
      Component::Normal(name) => match name.to_str() {
        Some(name) => names.push(name.to_owned()),
        None => {
          return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path is not valid UTF-8",
          ));
        }
      },
      Component::Prefix(_) => {
        return Err(io::Error::new(
          io::ErrorKind::InvalidInput,
          "path carries a drive prefix",
        ));
      }
    }
  }
  Ok(names)
}

/// Splits a path into the directories leading to an entry and its name.
pub fn split_parent(path: &Path) -> io::Result<(Vec<String>, String)> {
  let mut names = split_path(path)?;
  match names.pop() {
    Some(name) => Ok((names, name)),
    None => Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "path names the root itself, not an entry in it",
    )),
  }
}

/// Joins names back into the one spelling of their path.
pub fn join_names(names: &[String]) -> PathBuf {
  let mut path = PathBuf::from("/");
  path.extend(names);
  path
}
