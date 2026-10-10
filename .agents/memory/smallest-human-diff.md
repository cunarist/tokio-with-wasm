---
description: Changes are the smallest correct diff, written like a careful human
---

Prefer deleting to adding. No helpers or types with a single use, no defensive branches for impossible states, no comments that restate the code, no duplicated logic. Earlier AI-written work grew the crate threefold this way and was reverted; see [[reverted-0-10]].
