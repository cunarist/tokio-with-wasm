---
name: releasing
description: Release a new version through a GitHub release page, which tags it and triggers the crates.io publish. Use when cutting a release.
---

# Releasing

Ask the maintainer before any step that publishes.

1. Bump `version` in `package/Cargo.toml` and `package_proc/Cargo.toml`, and the `tokio_with_wasm_proc` dependency, to the same version. Merge that through a PR.
2. Create the release page, which also pushes the tag:

   ```sh
   gh release create vX.Y.Z --target main --generate-notes --latest
   ```

3. Watch the `Publish` workflow, which publishes the proc macro crate first. If only the main crate failed, publish it by hand, as a re-run fails on the proc macro crate.

Never push a bare `v*` tag: the release page is where users read what changed.
