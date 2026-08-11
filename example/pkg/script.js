import init, { render_fractal_frame } from "./example.js";

// Called from Rust through `wasm_bindgen`; the DOM is JavaScript's job.
// Defined before `init`, because the wasm start function already logs.
globalThis.appendLog = (message) => {
  const paragraph = document.createElement("p");
  paragraph.textContent = message;
  document.body.appendChild(paragraph);
};

// Fetches and instantiates the wasm module, then runs its start
// function, which spawns the async Rust checks that log below.
await init();

// Both rates on top of the fractal: the webview one proves that
// the main thread never freezes while the workers grind away.
let webviewFps = 0;
let fractalFps = 0;
function updateStats() {
  document.getElementById("stats").textContent =
    `webview ${webviewFps}fps, fractal ${fractalFps}fps`;
}

// Measures how fast the browser paints, by counting
// `requestAnimationFrame` callbacks over half-second windows.
// If the main thread were blocked, this number would collapse.
let paintCount = 0;
let paintStart = null;
function tick(now) {
  if (paintStart === null) paintStart = now;
  paintCount += 1;
  if (now - paintStart >= 500) {
    webviewFps = Math.round((paintCount * 1000) / (now - paintStart));
    paintCount = 0;
    paintStart = now;
    updateStats();
  }
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

// The animation loop; Rust only computes the pixels,
// spreading each frame over parallel web workers.
// A few frames stay in flight at once, so the workers keep
// rendering upcoming frames while an earlier one is drawn.
const FRAMES_IN_FLIGHT = 8;
const MIN_FRAME_MILLISECONDS = 1000 / 30;
async function animate() {
  const canvas = document.getElementById("fractal");
  const context = canvas.getContext("2d");
  let scale = 3.0;
  let frameCount = 0;
  let secondStart = performance.now();
  let nextDraw = performance.now();
  const pending = [];
  while (true) {
    while (pending.length < FRAMES_IN_FLIGHT) {
      pending.push(render_fractal_frame(scale));
      // Zoom in a little on every frame. Deeper zooms need more
      // iterations per pixel and get slow, so start over early.
      scale *= 0.96;
      if (scale < 3e-7) scale = 3.0;
    }
    const pixels = await pending.shift();
    const wait = nextDraw - performance.now();
    if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
    nextDraw = Math.max(nextDraw + MIN_FRAME_MILLISECONDS, performance.now());
    const image = new ImageData(
      new Uint8ClampedArray(pixels.buffer, pixels.byteOffset, pixels.length),
      canvas.width,
      canvas.height,
    );
    context.putImageData(image, 0, 0);

    frameCount += 1;
    const now = performance.now();
    if (now - secondStart >= 1000) {
      fractalFps = frameCount;
      frameCount = 0;
      secondStart = now;
      updateStats();
    }
  }
}

// Not awaited, so that module evaluation finishes
// and the page's load event can fire.
animate();
