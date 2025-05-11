use web_sys::window;

/// Checks if the current thread is the main JS thread,
/// not a web worker.
pub fn is_main_thread() -> bool {
  // Check if we have access to the window object.
  window().is_some()
}
