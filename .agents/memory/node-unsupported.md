---
description: Node isn't a supported runtime; only web browsers are
---

This crate targets web browsers on `wasm32-unknown-unknown`. Node-only behaviour, such as `setTimeout` returning an object or timers keeping the process alive, isn't a concern. Node users can run native `tokio`, or `wasm32-wasip1` through Node's WASI support.
