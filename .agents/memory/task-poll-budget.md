---
description: A spawned task that polls 128 times without the browser getting a turn waits for one macrotask
---

`spawn` in `package/src/glue/task/mod.rs` wraps each task in a poll budget
(#78).

Why it exists: woken tasks run as microtasks, and the browser drains every
microtask before it runs a macrotask (a timer, I/O callback, input event or a
frame). A task that keeps waking itself, such as two futures in one `join!`
passing messages, never lets the queue empty, so timers never fire and the
page freezes. With `+atomics`, `wasm_bindgen_futures` re-polls a task that wakes
itself inline, in the same microtask; without `+atomics`, any wake queues a
microtask. Both starve the browser the same way.

How it works: each task counts its polls. At poll 64 it arms a `setTimeout(0)`
probe that resets the count. The probe only runs once the browser gets a turn,
so a count of 128 means the browser hasn't had one since; the task then
returns `Pending` and is woken by a `setTimeout(0)`, so the browser runs at
least one macrotask. 128 follows tokio's coop budget; arming the probe at 64,
half of it, leaves slack for the probe to fire before the limit. The probe is
never cleared: it fires on the browser's next turn and only touches a `Cell`,
so it is harmless after the task ends. The count is per task, so a busy task
never holds back others (a per-thread count did, in round 1 of the review).

Why a probe and not "yield every 128 polls": the plain version also holds back
tasks that poll often but let the browser run in between, like a long stream. In
the review's benchmark it held normal work back 1,574 times and slowed a 300k
message stream from 1.3 s to 3.9 s. The probe held back 0 times in every
benchmark, so normal workloads never reach the limit.

Why tokio doesn't need this: it owns its event loop and checks timers and I/O
every 61 polls (`event_interval`), whatever tasks do. Here the browser owns the
loop and only gets it back when the microtask queue is empty.

Known limit: a loop whose futures are always ready never returns `Pending`, so
the wrapper is polled only once and the loop still freezes the page.

The regression test is `rt_common::busy_task_lets_others_run`.
