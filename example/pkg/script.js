import init, { render_fractal_frame } from "./example.js";

// Called from Rust; defined before `init`, as the start function logs.
globalThis.appendLog = (message) => {
  const paragraph = document.createElement("p");
  paragraph.textContent = message;
  document.body.appendChild(paragraph);
};

await init();

// Paints per second; this would collapse if the main thread were blocked.
const stats = document.getElementById("stats");
let paints = 0;
function paint() {
  paints += 1;
  requestAnimationFrame(paint);
}
requestAnimationFrame(paint);
setInterval(() => {
  stats.textContent = `main thread ${paints}fps`;
  paints = 0;
}, 1000);

const context = document.getElementById("fractal").getContext("2d");
let scale = 3.0;
async function draw() {
  const pixels = await render_fractal_frame(scale);
  const image = new ImageData(new Uint8ClampedArray(pixels.buffer), 320);
  context.putImageData(image, 0, 0);
  // Zoom in, starting over before the fixed iteration count runs out.
  scale = scale < 1e-5 ? 3.0 : scale * 0.96;
  requestAnimationFrame(draw);
}
draw();
