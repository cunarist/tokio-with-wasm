use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Expr, ItemFn, Lit, Meta, Path, Token, parse_macro_input};

/// Checks the attribute arguments that real `tokio` macros accept and
/// returns the path that the expansion should reference this crate by.
/// The runtime arguments configure a native runtime that doesn't exist
/// on the web, so their values are ignored; `crate = "..."` renames the
/// expansion path for dependencies renamed in `Cargo.toml`; unknown
/// arguments are an error, so that typos don't silently pass on the web
/// target only.
fn check_macro_args(
  args: &Punctuated<Meta, Token![,]>,
) -> Result<Path, syn::Error> {
  const IGNORED: &[&str] = &[
    "flavor",
    "worker_threads",
    "start_paused",
    "unhandled_panic",
  ];
  let mut crate_path: Path = syn::parse_quote!(tokio_with_wasm);
  for meta in args {
    let Some(ident) = meta.path().get_ident() else {
      return Err(unknown_argument_error(meta));
    };
    if ident == "crate" {
      let string = match meta {
        Meta::NameValue(pair) => match &pair.value {
          Expr::Lit(expr) => match &expr.lit {
            Lit::Str(string) => Some(string),
            _ => None,
          },
          _ => None,
        },
        _ => None,
      };
      let Some(string) = string else {
        return Err(syn::Error::new_spanned(
          meta,
          "`crate` expects a string literal, like `crate = \"my_alias\"`",
        ));
      };
      crate_path = string.parse()?;
    } else if !IGNORED.contains(&ident.to_string().as_str()) {
      return Err(unknown_argument_error(meta));
    }
  }
  Ok(crate_path)
}

fn unknown_argument_error(meta: &Meta) -> syn::Error {
  syn::Error::new_spanned(
    meta,
    "unknown attribute argument; expected one of: `flavor`, \
     `worker_threads`, `start_paused`, `unhandled_panic`, `crate`",
  )
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
  let crate_path = match check_macro_args(&args) {
    Ok(crate_path) => crate_path,
    Err(error) => return error.to_compile_error().into(),
  };

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
      #crate_path::spawn_local(async {
        #crate_path::MacroOutcome::handle(original().await);
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
  let crate_path = match check_macro_args(&args) {
    Ok(crate_path) => crate_path,
    Err(error) => return error.to_compile_error().into(),
  };

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

      #crate_path::MacroOutcome::handle(original().await);
    }
  };

  TokenStream::from(expanded)
}
