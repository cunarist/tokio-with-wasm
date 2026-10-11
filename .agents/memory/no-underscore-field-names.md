---
description: Field names never start with an underscore
---

The maintainer considers `_field` a convention from other languages, such as
Python's private names, and doesn't want it here. A field that only keeps
something alive is still named plainly (`closure`, not `_closure`) and is read
somewhere, so the compiler doesn't flag it as unused.
