#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
pub mod printing {
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn log(s: &str);
    }

    pub fn do_printing(s: &str) {
        log(s);
    }
}

#[cfg(not(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
)))]
pub mod printing {
    pub fn do_printing(s: &str) {
        println!("{}", s);
    }
}

#[macro_export]
macro_rules! print_fit {
    ($($t:tt)*) => {
        $crate::output::printing::do_printing(&format!($($t)*));
    };
}
