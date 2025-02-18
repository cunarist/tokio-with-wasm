mod entry;
mod output;

use entry::async_main;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(start)]
fn main() {
    async_main();
}
