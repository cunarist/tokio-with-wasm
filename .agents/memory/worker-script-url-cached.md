---
description: The worker script blob URL is made once at the first worker and never revoked, for WebKit
---

Every worker runs the same script, so `pool.rs` keeps one blob URL for it in a `OnceCell`, created with the first worker. It is fixed from then on, so `set_path_provider` must be called before the first `spawn_blocking`; later calls are ignored, which the maintainer accepted. The URL is never revoked, because WebKit may fetch the script after `new Worker` (vitejs/vite#19238, fixed in vite#19565); revoking on the worker's first reply worked but took more code (#75).
