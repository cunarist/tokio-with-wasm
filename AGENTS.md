At the start of every new session, run `damem recall` in the terminal and treat
its output as the context for this repository. If the `damem` command is not
found, read [INSTALL.md](https://github.com/cunarist/damem/blob/main/INSTALL.md)
and follow it before reasoning or responding.

# AGENTS.md

## Project

- **tokio_with_wasm** — `tokio`'s API for web browsers, on the JavaScript event loop and web workers
- Library (`package`), proc macros (`package_proc`), demo (`example`)

## Rules

- Every bug fix comes with a regression test that fails without the fix
- Tests follow `tokio`'s: one module per area, named like tokio's test files, porting tokio's tests where the web allows; web-only cases go in the same module
- Run tests with `wasm-pack test --headless --chrome package --lib --test '*'`
- Builds use nightly with `build-std` for every target, so native builds rebuild `std` too
