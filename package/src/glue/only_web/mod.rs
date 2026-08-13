//! Functions specific to WebAssembly web targets.
//! These functions are only available when compiling for the `wasm` family.

#[cfg(feature = "rt")]
mod path_provider;
#[cfg(feature = "rt")]
mod worker_script;

#[cfg(feature = "rt")]
pub use path_provider::*;
#[cfg(feature = "rt")]
pub use worker_script::*;
