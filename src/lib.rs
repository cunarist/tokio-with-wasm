#[cfg(not(target_family = "wasm"))]
pub use tokio;
#[cfg(target_family = "wasm")]
pub mod tokio;

#[cfg(target_family = "wasm")]
mod common;
