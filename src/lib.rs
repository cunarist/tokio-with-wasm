mod common;

#[cfg(not(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
)))]
compile_error!("`tokio_with_wasm` only supports `wasm32-unknown-unknown`");

#[cfg(feature = "macros")]
pub use tokio::join;
#[cfg(feature = "macros")]
pub use tokio::pin;
#[cfg(feature = "macros")]
pub use tokio::select;
#[cfg(feature = "macros")]
pub use tokio::try_join;

#[cfg(feature = "rt")]
pub mod task;
#[cfg(feature = "rt")]
pub use task::spawn;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "sync")]
pub use tokio::sync;
