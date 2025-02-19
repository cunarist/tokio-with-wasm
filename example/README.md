## Commands

To install dependencies:

```shell
cargo install wasm-pack
cargo install miniserve
```

To compile:

```shell
export RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals"
export RUSTUP_TOOLCHAIN="nightly"
wasm-pack build . --target web -- -Z build-std=std,panic_abort
```

To run on a web browser:

```shell
miniserve pkg --index index.html --header "Cross-Origin-Opener-Policy: same-origin" --header "Cross-Origin-Embedder-Policy: require-corp"
```

> You need to temporarily modify `.cargo/config.toml` to make `cargo run` use the native platform.
