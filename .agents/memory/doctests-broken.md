---
description: Doc examples in 0.9 never compiled, so tests run with --lib --test '*'
---

The glue only exists on wasm, where doctests did not run, so the examples collected errors. `--lib --test '*'` runs the unit and integration tests without them.
