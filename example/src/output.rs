#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
pub mod printing {
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = globalThis, js_name = eval)]
        fn eval(script: &str);
    }

    pub fn do_printing(s: &str) {
        let script = format!("document.body.innerHTML += '<p>{s}</p>';",);
        eval(&script);
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
