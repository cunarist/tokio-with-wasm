#[cfg(feature = "macros")]
pub use tokio::macros;

#[cfg(feature = "rt")]
pub mod task;
#[cfg(feature = "rt")]
pub use task::spawn;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "sync")]
pub use tokio::sync;
