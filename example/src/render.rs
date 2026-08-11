//! The fractal rendering entry point called from JavaScript.
//! Rust only computes pixels here; the page's JavaScript owns
//! the canvas, the animation loop, and the frame statistics.

use crate::fractal;
use tokio::task::spawn_blocking;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::prelude::wasm_bindgen;

/// The square frame's side length in pixels.
/// The `<canvas>` element in `index.html` matches this size.
const SIDE: u32 = 320;
/// How many strips a frame is split into,
/// which is also how many web workers render in parallel.
const STRIPS: u32 = 8;

/// Renders one full frame as RGBA pixels, with every strip
/// computed on its own web worker through `spawn_blocking`.
/// The returned buffer is copied to the JavaScript side,
/// so it can feed `ImageData` directly.
#[wasm_bindgen]
pub async fn render_fractal_frame(scale: f64) -> Vec<u8> {
  let strip_height = SIDE / STRIPS;
  let mut strips = Vec::with_capacity(STRIPS as usize);
  for index in 0..STRIPS {
    let y_start = index * strip_height;
    strips.push(spawn_blocking(move || {
      fractal::render_strip(SIDE, SIDE, y_start, strip_height, scale)
    }));
  }
  let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
  for strip in strips {
    let rendered = strip.await.expect("a fractal strip failed to render");
    pixels.extend_from_slice(&rendered);
  }
  pixels
}
