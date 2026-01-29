//! Functions specific to WebAssembly web targets.
//! These functions are only available when compiling for `wasm32` arch.

#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
mod path_provider;

#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
pub use path_provider::*;
