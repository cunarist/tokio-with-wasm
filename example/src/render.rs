//! Mandelbrot frames for the page, rendered on web workers.

use tokio::task::spawn_blocking;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::prelude::wasm_bindgen;

/// Matches the `<canvas>` size in `index.html`.
const SIDE: u32 = 320;
/// Each strip of a frame renders on its own web worker.
const STRIPS: u32 = 8;
const MAX_ITERATIONS: u32 = 500;
/// The "seahorse valley", where the page zooms in.
const CENTER: (f64, f64) = (-0.743_643_887_037_151, 0.131_825_904_205_330);

/// Renders one frame as RGBA pixels.
#[wasm_bindgen]
pub async fn render_fractal_frame(scale: f64) -> Vec<u8> {
  let rows = SIDE / STRIPS;
  let strips: Vec<_> = (0..STRIPS)
    .map(|i| spawn_blocking(move || render_strip(i * rows, rows, scale)))
    .collect();
  let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
  for strip in strips {
    pixels.extend(strip.await.unwrap());
  }
  pixels
}

fn render_strip(top: u32, rows: u32, scale: f64) -> Vec<u8> {
  let mut pixels = Vec::with_capacity((SIDE * rows * 4) as usize);
  for py in top..top + rows {
    for px in 0..SIDE {
      let cx = CENTER.0 + (px as f64 / SIDE as f64 - 0.5) * scale;
      let cy = CENTER.1 + (py as f64 / SIDE as f64 - 0.5) * scale;
      let (mut x, mut y, mut i) = (0.0, 0.0, 0);
      while x * x + y * y <= 4.0 && i < MAX_ITERATIONS {
        (x, y) = (x * x - y * y + cx, 2.0 * x * y + cy);
        i += 1;
      }
      let shade = (i % 64 * 4) as u8;
      pixels.extend(match i {
        MAX_ITERATIONS => [0, 0, 0, 255],
        _ => [shade, shade / 2, 255 - shade, 255],
      });
    }
  }
  pixels
}
