//! JavaScript glue module that mimics `tokio`.

mod common;

#[cfg(feature = "macros")]
pub use tokio::{join, pin, select, try_join};

#[cfg(feature = "sync")]
pub use tokio::sync;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "rt")]
pub mod task;
#[cfg(feature = "rt")]
pub use task::spawn;

#[cfg(all(any(feature = "rt", feature = "rt-multi-thread"), feature = "macros"))]
pub use tokio_with_wasm_proc::main;
#[doc(hidden)]
#[cfg(all(any(feature = "rt", feature = "rt-multi-thread"), feature = "macros"))]
pub use wasm_bindgen_futures::spawn_local;
