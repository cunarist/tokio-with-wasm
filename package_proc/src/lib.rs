use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{ItemFn, Meta, Token, parse_macro_input};

/// Checks the attribute arguments that real `tokio` macros accept.
/// They configure a native runtime that doesn't exist on the web,
/// so their values are ignored; unknown arguments are an error,
/// so that typos don't silently pass on the web target only.
fn check_macro_args(
  args: &Punctuated<Meta, Token![,]>,
) -> Result<(), syn::Error> {
  const KNOWN: &[&str] = &[
    "flavor",
    "worker_threads",
    "start_paused",
    "unhandled_panic",
    "crate",
  ];
  for meta in args {
    let is_known = meta
      .path()
      .get_ident()
      .is_some_and(|ident| KNOWN.contains(&ident.to_string().as_str()));
    if !is_known {
      return Err(syn::Error::new_spanned(
        meta,
        "unknown attribute argument; expected one of: `flavor`, \
         `worker_threads`, `start_paused`, `unhandled_panic`, `crate`",
      ));
    }
  }
  Ok(())
}

/// Attribute macro that mimics `tokio::main`.
/// This macro writes a function that simply spawns the given future
/// inside the JavaScript environment.
/// To execute the function, you might need to use
/// `#[wasm_bindgen(start)]` in addition to this macro.
///
/// Runtime arguments such as `flavor` and `worker_threads` are accepted
/// for compatibility and ignored, because the JavaScript event loop
/// replaces the runtime they would configure.
/// A `Result` returned from the function is unwrapped once the future
/// completes, turning an `Err` into a panic the way a native binary
/// exits with an error.
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
  let args = parse_macro_input!(
    attr with Punctuated::<Meta, Token![,]>::parse_terminated
  );
  if let Err(error) = check_macro_args(&args) {
    return error.to_compile_error().into();
  }

  // Parse the input tokens as a function
  let input_fn = parse_macro_input!(item as ItemFn);

  // Extract function components
  let attrs = &input_fn.attrs;
  let vis = &input_fn.vis;
  let fn_name = &input_fn.sig.ident;
  let fn_args = &input_fn.sig.inputs;
  let fn_block = &input_fn.block;
  let return_type = &input_fn.sig.output;

  // Generate a non-async function
  // that calls the original function with `spawn_local`
  let expanded = quote! {
    #(#attrs)*
    #vis fn #fn_name() {
      async fn original(#fn_args) #return_type #fn_block

      // Spawn the async function in a local task
      tokio_with_wasm::spawn_local(async {
        tokio_with_wasm::MacroOutcome::handle(original().await);
      });
    }
  };

  TokenStream::from(expanded)
}

/// Attribute macro that mimics `tokio::test`.
/// This macro writes an async `wasm-bindgen-test` test,
/// so the test crate must depend on `wasm-bindgen-test`.
///
/// Runtime arguments such as `flavor` and `start_paused` are accepted
/// for compatibility and ignored, because the JavaScript event loop
/// replaces the runtime they would configure.
/// A `Result` returned from the function is unwrapped once the future
/// completes, turning an `Err` into a test failure.
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
  let args = parse_macro_input!(
    attr with Punctuated::<Meta, Token![,]>::parse_terminated
  );
  if let Err(error) = check_macro_args(&args) {
    return error.to_compile_error().into();
  }

  // Parse the input tokens as a function
  let input_fn = parse_macro_input!(item as ItemFn);

  // Extract function components
  let attrs = &input_fn.attrs;
  let vis = &input_fn.vis;
  let fn_name = &input_fn.sig.ident;
  let fn_args = &input_fn.sig.inputs;
  let fn_block = &input_fn.block;
  let return_type = &input_fn.sig.output;

  // Generate an async test that the `wasm-bindgen-test` harness drives
  let expanded = quote! {
    #(#attrs)*
    #[::wasm_bindgen_test::wasm_bindgen_test]
    #vis async fn #fn_name() {
      async fn original(#fn_args) #return_type #fn_block

      tokio_with_wasm::MacroOutcome::handle(original().await);
    }
  };

  TokenStream::from(expanded)
}
