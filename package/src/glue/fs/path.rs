//! Path handling for the origin private file system.
//!
//! The origin private file system holds one root per origin and has no
//! symbolic links, so a path is nothing but the list of names that lead
//! from that root to an entry. A leading slash is accepted and ignored,
//! because there is no second root to be absolute against.

use std::io;
use std::path::{Component, Path, PathBuf};

/// Splits a path into the names that lead from the root to an entry.
///
/// `..` is resolved here rather than passed on. That is safe because the
/// origin private file system has no symbolic links, so no name can lead
/// somewhere other than where it reads.
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
          "path carries a drive prefix, \
           which the origin private file system has no room for",
        ));
      }
    }
  }
  Ok(names)
}

/// Splits a path into the directories that lead to an entry
/// and the name of the entry itself.
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

/// Joins names back into the one form that names an entry.
pub fn join_names(names: &[String]) -> PathBuf {
  let mut path = PathBuf::from("/");
  path.extend(names);
  path
}
