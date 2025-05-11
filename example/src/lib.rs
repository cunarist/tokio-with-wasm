mod entry;
mod measure;
mod output;

use entry::*;
use measure::*;

use wasm_bindgen::prelude::wasm_bindgen;

// On the web, this macro tells `wasm_bindgen`
// to run the function on JavaScript module initialization.
#[wasm_bindgen(start)]
fn main() {
  async_main();
}
