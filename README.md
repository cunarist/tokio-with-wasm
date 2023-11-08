# `tokio_with_wasm`

[![Crates.io](https://img.shields.io/crates/v/tokio_with_wasm.svg)](https://crates.io/crates/tokio_with_wasm)
[![Documentation](https://docs.rs/tokio_with_wasm/badge.svg)](https://docs.rs/tokio_with_wasm)
[![License](https://img.shields.io/crates/l/tokio_with_wasm.svg)](https://github.com/cunarist/tokio-with-wasm/blob/main/LICENSE)

![bandicam 2023-10-07 17-34-36-577](https://github.com/cunarist/tokio-with-wasm/assets/66480156/c2c97ce7-831e-4d4c-b960-f8f368e12c48)

> Tested with [Rinf](https://github.com/cunarist/rinf)

`tokio_with_wasm` is a Rust library that provides `tokio` specifically designed for web browsers. It aims to provide the exact same `tokio` features for web applications, leveraging JavaScript web API.

This library assumes that you're compilng your Rust project with `wasm-pack` and `wasm-bindgen`, which focuses on `wasm32-unknown-unknown` Rust target.

When using `spawn_blocking()`, the number of web workers are automatically adjusted adapting to the parallel tasks that has been queued by `spawn_blocking`. Refer to the docs for additional details.

On native platforms, `tokio_with_wasm::tokio` is the real `tokio` with `full` features, on the web, `tokio_with_wasm::tokio` is made up of JavaScript glue that mimics the behavior of real `tokio`.

## Features

- **Familiar API**: If you're familiar with `tokio`, you'll feel right at home with `tokio_with_wasm`. It provides similar functionality and follows the same patterns for spawning and managing asynchronous tasks.

- **Web Worker Integration**: `tokio_with_wasm` adapts to the JavaScript environment by utilizing web API under the hood. This means you can write Rust code that runs concurrently and efficiently in web applications.

- **Spawn Async and Blocking Tasks**: You can spawn both asynchronous and blocking tasks. Asynchronous tasks allow you to perform non-blocking operations, while blocking tasks are suitable for compute-heavy or synchronous tasks.

> Though various IO functionalities can be added in the future, they're not included yet.

## Why This is Needed

The web has many restrictions due to its sandboxed environment which prevents the use of threads, time, file IO, network IO, and many other native functionalities. Consequently, certain features are missing from Rust's `std` due to these limitations and `tokio` doesn't really work well on web browsers.

To address this issue, this crate offers `tokio` imports with the **same names** as the original native ones, providing workarounds for these constraints.

## Usage

Add this library to your `Cargo.toml`:

```toml
[dependencies]
tokio_with_wasm = "[latest-version]"
```

Here's a simple example of how to use `tokio_with_wasm`:

```rust
use tokio_with_wasm::{spawn, spawn_blocking, yield_now};

async fn start() {
    let async_join_handle = spawn(async {
        // Your asynchronous code here.
        // This will run concurrently
        // in the same web worker(thread).
    });
    let blocking_join_handle = spawn_blocking(|| {
        // Your blocking code here.
        // This will run parallelly
        // in the external pool of web workers.
    });
    let async_result = async_join_handle.await;
    let blocking_result = blocking_join_handle.await;
    for i in 1..1000 {
        // Some repeating task here
        // that shouldn't block the JavaScript runtime.
        yield_now().await;
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
You'll be surprised by various capabilities that modern JavaScript has. https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Functions_and_classes_available_to_workers. Also, there are many crates at `crates.io` that mimic native functionalities on the web.
Let's use them to avoid reinventing the wheels if needed.
