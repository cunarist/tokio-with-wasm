---
description: Each worker's script blob URL is revoked on its first reply, not right after `new Worker`, for WebKit
---

Every worker gets its own blob URL of the same script. Chrome and Firefox read it during `new Worker`, but WebKit may fetch it later, so revoking right away can stop the worker from loading (vitejs/vite#19238, fixed in vite#19565). The worker's first `message` only comes after its script ran a task, so `pool.rs` revokes the URL then (#75).
