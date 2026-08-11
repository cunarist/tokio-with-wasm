//! CPU-heavy Mandelbrot rendering, used as the blocking workload.
//!
//! Adapted from https://github.com/abour/fractal,
//! zooming into a different location of the set.

/// Where the animation zooms in: the "seahorse valley"
/// between the main cardioid and the period-2 bulb.
pub const CENTER_X: f64 = -0.743_643_887_037_151;
pub const CENTER_Y: f64 = 0.131_825_904_205_330;

/// How many iterations to spend on a pixel.
/// Deeper zooms need more to keep the boundary detailed,
/// which also makes each frame heavier: exactly the kind of
/// growing CPU load this demo wants to show.
pub fn max_iterations(scale: f64) -> u32 {
  let zoom_decades = (3.0 / scale).log10().max(0.0);
  (100.0 + 16.0 * zoom_decades * zoom_decades) as u32
}

/// Renders a horizontal strip of the Mandelbrot set as RGBA pixels.
/// This is pure, synchronous math that keeps a web worker busy.
pub fn render_strip(
  width: u32,
  height: u32,
  y_start: u32,
  strip_height: u32,
  scale: f64,
) -> Vec<u8> {
  let max_iter = max_iterations(scale);
  let mut pixels = Vec::with_capacity((width * strip_height * 4) as usize);
  for py in y_start..y_start + strip_height {
    for px in 0..width {
      let real = CENTER_X + (px as f64 / width as f64 - 0.5) * scale;
      let imag = CENTER_Y + (py as f64 / height as f64 - 0.5) * scale;
      let (r, g, b) = color_at(real, imag, max_iter);
      pixels.extend_from_slice(&[r, g, b, 255]);
    }
  }
  pixels
}

/// Escape-time iteration of `z -> z^2 + c` with smooth coloring.
fn color_at(real: f64, imag: f64, max_iter: u32) -> (u8, u8, u8) {
  let mut x = 0.0_f64;
  let mut y = 0.0_f64;
  let mut iterations = 0;
  while x * x + y * y <= 4.0 && iterations < max_iter {
    let next_x = x * x - y * y + real;
    y = 2.0 * x * y + imag;
    x = next_x;
    iterations += 1;
  }
  if iterations >= max_iter {
    // Inside the set.
    return (10, 10, 25);
  }
  // Fractional iteration count, so that color bands blend smoothly.
  let magnitude = (x * x + y * y).sqrt();
  let smooth =
    iterations as f64 + 1.0 - magnitude.ln().ln() / std::f64::consts::LN_2;
  hsl_to_rgb(smooth * 7.0 % 360.0, 0.85, 0.55)
}

/// Standard HSL to RGB conversion.
/// `hue` is in degrees; `saturation` and `lightness` are `0.0..=1.0`.
fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> (u8, u8, u8) {
  let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
  let secondary = chroma * (1.0 - (hue / 60.0 % 2.0 - 1.0).abs());
  let base = lightness - chroma / 2.0;
  let (r, g, b) = match hue as u32 / 60 {
    0 => (chroma, secondary, 0.0),
    1 => (secondary, chroma, 0.0),
    2 => (0.0, chroma, secondary),
    3 => (0.0, secondary, chroma),
    4 => (secondary, 0.0, chroma),
    _ => (chroma, 0.0, secondary),
  };
  (
    ((r + base) * 255.0) as u8,
    ((g + base) * 255.0) as u8,
    ((b + base) * 255.0) as u8,
  )
}
