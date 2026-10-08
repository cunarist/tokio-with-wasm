use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprLit, ItemFn, Lit, Meta, Path, Token, parse_macro_input};

/// Returns the path that the expansion should reference this crate by.
/// The runtime arguments of the real `tokio` macros configure a native
/// runtime that doesn't exist on the web, so they are accepted and ignored.
/// `crate = "..."` renames the path for dependencies renamed in
/// `Cargo.toml`. Unknown arguments are an error, so that typos don't
/// silently pass on the web target only.
fn crate_path(args: &Punctuated<Meta, Token![,]>) -> syn::Result<Path> {
  let mut path = syn::parse_quote!(tokio_with_wasm);
  for meta in args {
    let ident = meta.path().get_ident().map(ToString::to_string);
    match ident.as_deref() {
      Some(
        "flavor" | "worker_threads" | "start_paused" | "unhandled_panic",
      ) => {}
      Some("crate") => {
        let value = &meta.require_name_value()?.value;
        let Expr::Lit(ExprLit {
          lit: Lit::Str(string),
          ..
        }) = value
        else {
          return Err(syn::Error::new_spanned(value, "expected a string"));
        };
        path = string.parse()?;
      }
      _ => {
        return Err(syn::Error::new_spanned(
          meta,
          "unknown attribute argument; expected one of: `flavor`, \
           `worker_threads`, `start_paused`, `unhandled_panic`, `crate`",
        ));
      }
    }
  }
  Ok(path)
}

/// Writes `main` or `test` around the given async function.
fn expand(attr: TokenStream, item: TokenStream, test: bool) -> TokenStream {
  let args = parse_macro_input!(
    attr with Punctuated::<Meta, Token![,]>::parse_terminated
  );
  let crate_path = match crate_path(&args) {
    Ok(crate_path) => crate_path,
    Err(error) => return error.to_compile_error().into(),
  };
  let ItemFn {
    attrs,
    vis,
    sig,
    block,
  } = parse_macro_input!(item as ItemFn);
  let (name, inputs, output) = (&sig.ident, &sig.inputs, &sig.output);
  let original = quote! { async fn original(#inputs) #output #block };
  let handle = quote! { #crate_path::MacroOutcome::handle(original().await); };
  let expanded = if test {
    // An async test that the `wasm-bindgen-test` harness drives
    quote! {
      #(#attrs)*
      #[::wasm_bindgen_test::wasm_bindgen_test]
      #vis async fn #name() {
        #original
        #handle
      }
    }
  } else {
    // A non-async function that spawns the original one in a local task
    quote! {
      #(#attrs)*
      #vis fn #name() {
        #original
        #crate_path::spawn_local(async { #handle });
      }
    }
  };
  expanded.into()
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
  expand(attr, item, false)
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
  expand(attr, item, true)
}
