---
description: `Sleep` stays non-`Send` by choice; issue #33 was closed as not planned
---

A `Send` `Sleep` would need its JS timer to live somewhere that can't vanish.
A timer on the polling worker hangs once that worker is terminated after 10
seconds idle. Routing every timer through the main thread works but splits
each sleep into two paths, which the maintainer rejected as too complex
(2026-10-12). `unsafe impl Send` is unsound if a `Sleep` is dropped inside a
`spawn_blocking` closure. Async tasks only ever run on the main thread, since
`spawn` panics on workers. See [[unsafe-rarely-never-in-tests]].
