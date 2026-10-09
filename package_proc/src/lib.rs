use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprLit, ItemFn, Lit, Meta, Path, Token, parse_macro_input};

/// Returns the crate path set by `crate = "..."`.
/// Runtime arguments are accepted and ignored, as there is no runtime to configure.
fn crate_path(attr: TokenStream) -> syn::Result<Path> {
  let args = Punctuated::<Meta, Token![,]>::parse_terminated.parse(attr)?;
  let mut path = syn::parse_quote!(tokio_with_wasm);
  for meta in args {
    let value = &meta.require_name_value()?.value;
    match meta.path().get_ident().map(ToString::to_string).as_deref() {
      Some(
        "flavor" | "worker_threads" | "start_paused" | "unhandled_panic"
        | "name",
      ) => {}
      Some("crate") => match value {
        Expr::Lit(ExprLit {
          lit: Lit::Str(string),
          ..
        }) => path = string.parse()?,
        _ => return Err(syn::Error::new_spanned(value, "expected a string")),
      },
      _ => {
        return Err(syn::Error::new_spanned(
          meta,
          "unknown attribute; expected one of: `flavor`, `worker_threads`, \
           `start_paused`, `crate`, `unhandled_panic`, `name`",
        ));
      }
    }
  }
  Ok(path)
}

/// Mimics `tokio::main` by spawning the function's body on the JavaScript
/// event loop. You might need `#[wasm_bindgen(start)]` to call it.
/// An `Err` return panics.
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
  let crate_path = match crate_path(attr) {
    Ok(path) => path,
    Err(error) => return error.to_compile_error().into(),
  };
  let ItemFn {
    attrs,
    vis,
    sig,
    block,
  } = parse_macro_input!(item as ItemFn);
  let (name, output) = (&sig.ident, &sig.output);
  quote! {
    #(#attrs)*
    #vis fn #name() {
      async fn original() #output #block
      #crate_path::task::spawn_local(async {
        #crate_path::macros::Outcome::handle(original().await)
      });
    }
  }
  .into()
}

/// Mimics `tokio::test` with an async `wasm-bindgen-test` test,
/// so the test crate must depend on `wasm-bindgen-test`.
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
  if let Err(error) = crate_path(attr) {
    return error.to_compile_error().into();
  }
  let mut expanded =
    TokenStream::from(quote!(#[::wasm_bindgen_test::wasm_bindgen_test]));
  expanded.extend(item);
  expanded
}
