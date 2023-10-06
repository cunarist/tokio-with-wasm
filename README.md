# `async_wasm_task`

[![Crates.io](https://img.shields.io/crates/v/async_wasm_task.svg)](https://crates.io/crates/async_wasm_task)
[![Documentation](https://docs.rs/async_wasm_task/badge.svg)](https://docs.rs/async_wasm_task)
[![License](https://img.shields.io/crates/l/async_wasm_task.svg)](https://github.com/cunarist/async-wasm-task/blob/main/LICENSE)

`async_wasm_task` is a Rust library that provides an API for managing asynchronous tasks in a JavaScript environment, closely resembling the familiar patterns of `tokio::task`. It is designed to allow Rust code to work seamlessly with JavaScript in web applications, leveraging web workers for concurrent task execution.

## Features

- **Familiar API**: If you're familiar with `tokio::task`, you'll feel right at home with `async_wasm_task`. It provides similar functionality and follows the same patterns for spawning and managing asynchronous tasks.

- **Web Worker Integration**: `async_wasm_task` adapts to the JavaScript environment by utilizing web workers under the hood. This means you can write Rust code that runs concurrently and efficiently in web applications.

- **Spawn Async and Blocking Tasks**: You can spawn both asynchronous and blocking tasks. Asynchronous tasks allow you to perform non-blocking operations, while blocking tasks are suitable for compute-heavy or synchronous tasks.

## Usage

Add this library to your `Cargo.toml`:

```toml
[dependencies]
async_wasm_task = "0.1"
```

Here's a simple example of how to use `async_wasm_task`:

```rust
use async_wasm_task::{spawn, spawn_blocking};

async fn main() {
    let async_join_handle = spawn(async {
        // Your asynchronous code here
    });
    let blocking_join_handle = spawn_blocking(|| {
        // Your blocking code here
    });
    let async_result = async_join_handle.await;
    let blocking_result = blocking_join_handle.await;
    // Handle the results after that...
}
```

## Documentation

Detailed documentation can be found on [docs.rs](https://docs.rs/async_wasm_task).

## Contributing

Contributions are welcome! If you have any suggestions, bug reports, or want to contribute to the development of `async_wasm_task`, please open an issue or submit a pull request.
