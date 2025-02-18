use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Attribute macro that mimics `tokio::main`.
/// This macro returns a function that simply spawns the given future
/// inside the JavaScript environment.
/// To execute the function, you might need to use
/// `#[wasm_bindgen(start)]` in addition to this macro.
#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input tokens as a function
    let input_fn = parse_macro_input!(item as ItemFn);

    // Extract function components
    let vis = &input_fn.vis;
    let fn_name = &input_fn.sig.ident;
    let fn_args = &input_fn.sig.inputs;
    let fn_block = &input_fn.block;
    let return_type = &input_fn.sig.output;

    // Generate a non-async function
    // that calls the original function with `spawn_local`
    let expanded = quote! {
        #vis fn #fn_name() {
            async fn original(#fn_args) #return_type #fn_block

            // Spawn the async function in a local task
            tokio_with_wasm::spawn_local(async {
                let _ = original().await;
            });
        }
    };

    TokenStream::from(expanded)
}
