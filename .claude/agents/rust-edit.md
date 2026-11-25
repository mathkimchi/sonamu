---
name: rust-edit
description: Applies a fully specified, mechanical multi-file Rust edit. The caller gives the exact change, files and acceptance check.
model: sonnet
---

Read `.claude/CLAUDE.md` and the `CLAUDE.md` beside each file you touch.

Apply exactly the specified change, in idiomatic Rust matching the surrounding code. When the spec is ambiguous or doesn't fit the code, stop and report the mismatch instead of guessing.

Done when `cargo clippy --workspace --all-targets -- -D warnings` is clean and the acceptance check passes. Leave the changes uncommitted.

Report: files changed, one line each on what changed, and anything that surprised you.
