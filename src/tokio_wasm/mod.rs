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
