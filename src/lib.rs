#[cfg(target_family = "wasm")]
mod common;
#[cfg(target_family = "wasm")]
pub mod tokio_wasm;

#[cfg(not(target_family = "wasm"))]
pub use tokio;
#[cfg(target_family = "wasm")]
pub use tokio_wasm as tokio;
