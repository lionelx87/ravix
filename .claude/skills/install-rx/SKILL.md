---
name: install-rx
description: Build Ravix in release mode and install it as the `rx` command on this machine, after the same checks CI runs. Use when finishing an implementation or fix in Ravix, or when asked to install or update `rx`.
---

Run `scripts/install.sh` from the repo root. It runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`, then `cargo install --path . --locked`, which replaces `~/.cargo/bin/rx`.

Pass `--skip-checks` only when fmt, clippy and the tests already passed in this task and no code changed afterwards.

If a check fails, stop: report the failing output to the user and do not install. Do not run a repo-wide `cargo fmt` to make it pass; fix only the files the current task touched.

When it succeeds, tell the user the commit that was installed (the script prints it) and whether the build included uncommitted changes.
