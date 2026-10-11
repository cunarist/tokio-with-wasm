---
description: Workers share one cached script blob URL per path that is never revoked, for WebKit
---

Every worker runs the same script, so `pool.rs` keeps one blob URL for it and makes a new one only when the script path changes. Revoking a worker's URL right after `new Worker` breaks WebKit, which may fetch the script later (vitejs/vite#19238, fixed in vite#19565), and revoking on the worker's first reply took more code than one small blob per path kept for the page's lifetime (#75).
