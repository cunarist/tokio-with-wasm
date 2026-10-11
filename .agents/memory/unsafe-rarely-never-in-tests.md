---
description: `unsafe` is a last resort in library code and never appears in tests
---

The maintainer wants `unsafe` used very rarely, and only with care, where no safe
option exists. Tests never use it: a race that only an `unsafe` hand-made waker can
hit goes untested rather than tested that way (see #70).
