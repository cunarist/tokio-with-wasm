mod entry;
mod fractal;
mod measure;
mod output;
mod render;

use entry::*;
use measure::*;

use wasm_bindgen::prelude::wasm_bindgen;

// On the web, this macro tells `wasm_bindgen`
// to run the function on JavaScript module initialization.
#[wasm_bindgen(start)]
fn main() {
  // Panics cannot unwind on the web; at least make them readable.
  std::panic::set_hook(Box::new(|info| {
    print_fit!("{info}");
  }));
  async_main();
}
