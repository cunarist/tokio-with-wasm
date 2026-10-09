//! JavaScript glue module that mimics `tokio`.

mod common;

#[cfg(feature = "fs")]
pub mod fs;

// Readers and writers don't touch the OS, so real `tokio::io` works as is.
pub use tokio::{io, pin};

#[cfg(feature = "macros")]
pub use tokio::{join, select, try_join};

#[cfg(feature = "sync")]
pub use tokio::sync;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "rt")]
pub mod task;
#[cfg(feature = "rt")]
pub use task::spawn;
#[cfg(feature = "rt")]
pub(crate) use task::*;
// Its machinery doesn't depend on the `tokio` runtime.
#[cfg(feature = "rt")]
pub use tokio::task_local;

#[cfg(feature = "macros")]
pub use tokio_with_wasm_proc::{main, test};

// What `#[tokio::main]` expands to. It needs `rt`, like `tokio`'s, and
// panics on `Err` like a native `main` exits with one.
#[doc(hidden)]
#[cfg(feature = "macros")]
pub mod macros {
  // Not `task::spawn_local`, which is main-thread only: `start` functions
  // may run on worker threads too.
  #[cfg(feature = "rt")]
  pub use wasm_bindgen_futures::spawn_local;

  pub trait Outcome {
    fn handle(self);
  }

  impl Outcome for () {
    fn handle(self) {}
  }

  impl<T, E: std::fmt::Debug> Outcome for Result<T, E> {
    fn handle(self) {
      if let Err(error) = self {
        panic!("the function returned an error: {error:?}");
      }
    }
  }
}

#[allow(unused_imports)]
pub(crate) use common::*;

#[cfg(feature = "rt")]
pub mod only_web;
