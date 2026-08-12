//! Functions specific to WebAssembly web targets.
//! These functions are only available when compiling for the `wasm` family.

#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
mod path_provider;
#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
mod worker_script;

#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
pub use path_provider::*;
#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
pub use worker_script::*;
