//! Task identifiers, mirroring `tokio::task::Id`.

use std::cell::Cell;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

thread_local! {
  /// The identifier handed to the next spawned task.
  /// Tasks are only spawned from the main thread,
  /// so a thread-local counter never hands out duplicates.
  static NEXT_TASK_ID: Cell<NonZeroU64> = const { Cell::new(NonZeroU64::MIN) };
  /// The identifier of the task being polled right now.
  /// Each web worker has its own slot,
  /// which is set while a blocking task runs there.
  static CURRENT_TASK_ID: Cell<Option<Id>> = const { Cell::new(None) };
}

/// An opaque ID that uniquely identifies a task relative to all other
/// currently running tasks.
///
/// # Notes
///
/// - Task IDs are unique relative to other *currently running* tasks.
///   When a task completes, the same ID may be used for another task.
/// - Task IDs are not guaranteed to be sequential, and do not indicate the
///   order in which tasks are spawned or any other data.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct Id(NonZeroU64);

impl Id {
  /// Takes the next free task identifier.
  pub(crate) fn next() -> Self {
    Id(NEXT_TASK_ID.replace(NEXT_TASK_ID.get().saturating_add(1)))
  }
}

impl Display for Id {
  fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
    self.0.fmt(fmt)
  }
}

/// Returns the [`Id`] of the currently running task.
///
/// # Panics
///
/// This function panics if called from outside a task.
pub fn id() -> Id {
  match try_id() {
    Some(id) => id,
    None => panic!("can't get a task ID when not inside a task"),
  }
}

/// Returns the [`Id`] of the currently running task, or `None` if called
/// outside of a task.
pub fn try_id() -> Option<Id> {
  CURRENT_TASK_ID.with(|cell| cell.get())
}

/// Runs `callable` with [`try_id`] reporting `id`,
/// the way a task observes its own identifier.
/// This wraps a single poll of a spawned task,
/// or the whole closure of a blocking one.
pub(crate) fn scope<T>(id: Id, callable: impl FnOnce() -> T) -> T {
  let previous = CURRENT_TASK_ID.replace(Some(id));
  let returned = callable();
  CURRENT_TASK_ID.set(previous);
  returned
}

#[cfg(test)]
mod tests {
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn try_id_is_none_outside_of_tasks() {
    assert_eq!(try_id(), None);
  }

  #[wasm_bindgen_test]
  fn scope_restores_the_previous_id() {
    let id = Id::next();
    let observed = scope(id, try_id);
    assert_eq!(observed, Some(id));
    assert_eq!(try_id(), None);
  }
}
