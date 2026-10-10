---
description: Panics are a documented web limitation; a panicking task hangs, by choice
---

The README says never to panic in `spawn` or `spawn_blocking`, since wasm can't unwind. A panic traps, so no destructors run, and the task's `JoinHandle` never resolves.

- In `spawn`, the future's memory leaks.
- In `spawn_blocking`, the worker also keeps its pool slot, so after 512 panics every later `spawn_blocking` hangs.
- Workarounds for the trap (a worker-script rethrow, `__wbindgen_thread_destroy`, a panic hook) were judged not worth the code. The plan is real unwinding with `-Cpanic=unwind` (#67).
