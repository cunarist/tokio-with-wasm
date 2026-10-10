---
description: The public API matches tokio exactly, feature gates included
---

Code written against `tokio_with_wasm::alias` must compile and behave the same on native `tokio`. Keep every feature, but use tokio's paths, signatures, semantics, and feature gates; `JoinMap` follows `tokio_util`.
