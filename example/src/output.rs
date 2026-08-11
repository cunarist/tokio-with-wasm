#[cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]
pub mod printing {
  use wasm_bindgen::prelude::wasm_bindgen;

  #[wasm_bindgen]
  extern "C" {
    /// Defined in `index.html`; appends a paragraph to the page.
    /// The DOM stays JavaScript's job.
    #[wasm_bindgen(js_namespace = globalThis, js_name = appendLog)]
    fn append_log(message: &str);
  }

  pub fn do_printing(s: &str) {
    append_log(s);
  }
}

#[cfg(not(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
)))]
pub mod printing {
  pub fn do_printing(s: &str) {
    println!("{s}");
  }
}

#[macro_export]
/// Prints to the HTML document when compiled to WASM.
/// Otherwise, it prints to `stdout`.
macro_rules! print_fit {
    ($($t:tt)*) => {
        $crate::output::printing::do_printing(&format!($($t)*))
    };
}
