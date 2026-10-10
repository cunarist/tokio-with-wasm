---
description: Panics are a documented web limitation; don't add more panic handling
---

The README says never to panic in `spawn` or `spawn_blocking`, since wasm can't unwind. A panic traps, so no destructors run.

- In `spawn`, the `JoinHandle` never resolves and the future's memory leaks. This is accepted.
- In `spawn_blocking`, the worker's `onerror` fails the handle and frees the slot. The worker's 2 MB stack, its TLS and the task's heap still leak. A lock held at the trap is never released.
- Freeing the stack with `__wbindgen_thread_destroy`, or failing tasks from a panic hook, was judged not worth the code.
