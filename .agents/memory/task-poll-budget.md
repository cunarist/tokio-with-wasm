---
description: A spawned task waits once every 128 polls so other Rust tasks run; browser turns are the user's job via yield_now
---

`spawn` in `package/src/glue/task/mod.rs` wraps each task in a poll budget
(#78). The regression test is `rt_common::busy_task_lets_others_run`.

Why: with `+atomics`, js-sys's executor (`src/futures/task/multithread.rs`,
`Task::run`) re-polls a task inline when it woke itself during its poll, so a
task whose futures keep waking each other (two futures in one `join!` passing
messages) never returns to the queue and starves every other task. Without
`+atomics` every wake queues the task, so tasks already take turns.

How: every 128 polls (tokio's coop budget) the wrapper returns `Pending` and
wakes the task from `queueMicrotask`. The wake comes after the poll, so the
executor resumes the task through `Atomics.waitAsync` and other tasks run first.

Policy: the budget only makes Rust tasks cooperate. Giving the browser a turn
(timers, input, rendering) is the user's job: busy loops call `yield_now`.
Chrome happens to run timers between the waitAsync resumes; Firefox doesn't, so
a self-waking task there still starves timers until it calls `yield_now`.

Why not `setTimeout`: an earlier version waited for a timer every 128 polls.
Nested timers clamp to 4 ms, so it needed a probe to skip the wait when the
browser had already had a turn, which was more code for a guarantee the
maintainer leaves to `yield_now`.

Known limit: a loop whose futures are always ready never returns `Pending`, so
the wrapper is polled once and the loop still blocks everything.
