# `tokio_with_wasm`

[![Crates.io](https://img.shields.io/crates/v/tokio_with_wasm.svg)](https://crates.io/crates/tokio_with_wasm)
[![Documentation](https://docs.rs/tokio_with_wasm/badge.svg)](https://docs.rs/tokio_with_wasm)
[![License](https://img.shields.io/crates/l/tokio_with_wasm.svg)](https://github.com/cunarist/tokio-with-wasm/blob/main/LICENSE)

![Recording](https://github.com/cunarist/tokio-with-wasm/assets/66480156/77fa5838-23c7-4e3b-b1ba-61146972c2aa)

> Tested with [Rinf](https://github.com/cunarist/rinf)

`tokio_with_wasm` is a Rust library that provides `tokio` specifically designed for web browsers. It aims to provide the exact same `tokio` features for web applications, leveraging JavaScript web API.

This library assumes that you're compilng your Rust project with `wasm-pack` and `wasm-bindgen`, which currently uses `wasm32-unknown-unknown` Rust target. Note that this library currently only supports the `web` target of `wasm-bindgen`, not [others](https://rustwasm.github.io/wasm-bindgen/reference/deployment.html) such as `no-modules`. Support for them can be added if there is a demand for it.

When using `spawn_blocking()`, the number of web workers are automatically adjusted adapting to the number of parallel tasks. Refer to the docs for additional details.

On native platforms, `tokio_with_wasm::tokio` is the real `tokio` with the `full` feature enabled. On the web, `tokio_with_wasm::tokio` is made up of JavaScript glue code that mimics the behavior of real `tokio`. Because `tokio_with_wasm` doesn't have its own runtime and adapts to the JavaScript event loop, advanced features of `tokio` might not work.

After building your webassembly module and preparing it for deployment, ensure that your web server is configured to include cross-origin-related HTTP headers in its responses. Set the [`cross-origin-opener-policy`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cross-Origin-Opener-Policy) to `same-origin` and [`cross-origin-embedder-policy`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cross-Origin-Embedder-Policy) to `require-corp`. These headers enable clients using your website to gain access to `SharedArrayBuffer` web API, which is something similar to shared memory on the web. Additionally, don't forget to specify the MIME type `application/wasm` for `.wasm` files within the server configurations to ensure optimal performance.

## Features

- **Familiar API**: If you're familiar with `tokio`, you'll feel right at home with `tokio_with_wasm`. It provides similar functionality and follows the same patterns for spawning and managing asynchronous tasks.

- **Web Worker Integration**: `tokio_with_wasm` adapts to the JavaScript environment by utilizing web API under the hood. This means you can write Rust code that runs concurrently and efficiently in web applications.

- **Spawn Async and Blocking Tasks**: You can spawn both asynchronous and blocking tasks. Asynchronous tasks allow you to perform non-blocking operations, while blocking tasks are suitable for compute-heavy or synchronous tasks.

> Though various IO functionalities can be added in the future, they're not included yet.

## Why This is Needed

The web has many restrictions due to its sandboxed environment which prevents the use of threads, time, file IO, network IO, and many other native functionalities. Consequently, certain features are missing from Rust's `std` due to these limitations. That's why `tokio` doesn't really work well on web browsers.

To address this issue, this crate offers `tokio` modules with the **same names** as the original native ones, providing workarounds for these constraints.

## Future Vision

Because a large portion of Rust's web ecosystem is based on `wasm32-unknown-unknown` right now, we had to make an alias crate of `tokio` to use its functionalities directly on the web.

Hopefully, when `wasm32-wasi` becomes the mainstream Rust target for the web, [`jco`](https://github.com/bytecodealliance/jco) might be an alternative to `wasm-bindgen` as it can provide full `std` functionalities with browser shims (polyfills). However, this will take time because the [`wasi-threads`](https://github.com/WebAssembly/wasi-threads) proposal still has a long way to go.

Until that time, there's `tokio_with_wasm`!

## Usage

Add this library to your `Cargo.toml`:

```toml
[dependencies]
tokio_with_wasm = "[latest-version]"
```

Here's a simple example of how to use `tokio_with_wasm`:

```rust
use tokio_with_wasm::tokio;

async fn start() {
    let async_join_handle = tokio::spawn(async {
        // Your asynchronous code here.
        // This will run concurrently
        // in the same web worker(thread).
    });
    let blocking_join_handle = tokio::task::spawn_blocking(|| {
        // Your blocking code here.
        // This will run parallelly
        // in the external pool of web workers.
    });
    let async_result = async_join_handle.await;
    let blocking_result = blocking_join_handle.await;
    for i in 1..1000 {
        // Some repeating task here
        // that shouldn't block the JavaScript runtime.
        tokio::task::yield_now().await;
    }
}
```

## Documentation

Detailed documentation can be found on [docs.rs](https://docs.rs/tokio_with_wasm).

## Contribution Guide

Contributions are always welcome! If you have any suggestions, bug reports, or want to contribute to the development of `tokio_with_wasm`, please open an issue or submit a pull request.

There are situations where you cannot use native Rust code directly on the web. This is because `wasm32-unknown-unknown` Rust target used by `wasm-bindgen` doesn't have a full `std` module. Refer to the links below to understand how to interact with JavaScript with `wasm-bindgen`.

- https://rustwasm.github.io/wasm-bindgen/reference/attributes/on-js-imports/js_name.html
- https://rustwasm.github.io/wasm-bindgen/reference/attributes/on-js-imports/js_namespace.html

We should always assume that Rust code is executed in a **web worker**. Therefore, we cannot access the global `window` JavaScript object
just like when you work in the main thread of JavaScript. Refer to the link below to check which web APIs are available in a web worker.
You'll be surprised by various capabilities that modern JavaScript has.

- https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Functions_and_classes_available_to_workers

Also, there are many crates at `crates.io` that mimic native functionalities on the web. Let's use them to avoid reinventing the wheels if needed.

Please note that this library uses a quite hacky and naive approach to mimic native `tokio` functionalities. That's because this library is regarded as a temporary solution for the period before `wasm32-wasi`. Any kind of PR is possible, as long as it makes things just work on the web.
