//! Functions specific to WebAssembly web targets.
//! These functions are only available when compiling for `wasm32` arch.

#[cfg(feature = "rt")]
mod path_provider;

#[cfg(feature = "rt")]
pub use path_provider::*;