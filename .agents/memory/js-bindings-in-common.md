---
description: All JS bindings live in the one `extern "C"` block in `common/mod.rs`, never inline
---

Strict rule from the maintainer: every `#[wasm_bindgen] extern "C"` binding goes in the shared block in `package/src/glue/common/mod.rs`, even if only one place uses it. If a binding is only needed in some builds, gate the binding itself there with `#[cfg]` rather than moving it next to its caller.
