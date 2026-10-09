## Commands

To install dependencies:

```shell
cargo install wasm-pack
cargo install miniserve
```

To compile, from this directory (the repository's `rust-toolchain.toml` and `.cargo/config.toml` supply the nightly toolchain and web worker flags):

```shell
wasm-pack build . --target web
```

To view in a browser:

```shell
miniserve pkg --index index.html --header "cross-origin-opener-policy:same-origin" --header "cross-origin-embedder-policy:require-corp"
```
