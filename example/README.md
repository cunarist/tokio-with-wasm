## Commands

To install dependencies:

```shell
cargo install wasm-pack
cargo install miniserve
```

To compile:

```shell
export RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--shared-memory -C link-arg=--max-memory=1073741824 -C link-arg=--import-memory -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base"
export RUSTUP_TOOLCHAIN="nightly"
wasm-pack build . --target web -- -Z build-std=std,panic_abort
```

To view in a browser:

```shell
miniserve pkg --index index.html --header "Cross-Origin-Opener-Policy:same-origin" --header "Cross-Origin-Embedder-Policy:require-corp"
```

> You need to temporarily modify `.cargo/config.toml` to make `cargo run` use the native platform, if you've directly cloned this repository.
