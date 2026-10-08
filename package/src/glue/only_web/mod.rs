//! Functions specific to WebAssembly web targets.
//! These functions are only available when compiling for the `wasm` family.

mod path_provider;
mod worker_script;

pub use path_provider::*;
pub use worker_script::*;
