---
description: `unsafe` is a rare last resort that needs the maintainer's consent, and never appears in tests
---

The maintainer wants `unsafe` used very rarely and carefully, only where no safe
option exists, and only after the maintainer agrees to it. Tests never use it: a
race that only an `unsafe` hand-made waker can hit goes untested rather than
tested that way (see #70).
