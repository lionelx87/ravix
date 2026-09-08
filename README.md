# Ravix

An animated git client for the terminal, written in Rust.

[![CI](https://github.com/lionelx87/ravix/actions/workflows/ci.yml/badge.svg)](https://github.com/lionelx87/ravix/actions/workflows/ci.yml)

The graph is home. Ravix renders the commit graph as pure terminal cell art —
truecolor, unicode curves, braille — and hangs contextual panels off it for the
working directory, branches, diffs, stashes and submodules. Keyboard-first, with
full mouse support including branch drag & drop.

> **Status:** personal daily driver, in testing. All v1 phases are complete and
> the suite is green, but there is no tagged release yet and the interface still
> moves. Use it on repositories you have pushed.

<!-- TODO: screenshot or asciinema recording of the graph view goes here -->

## What makes it different

**A hybrid backend, on purpose.** Reads go through `git2` (libgit2) — repository
discovery, the commit walk, ref badges, status, per-file diffs. Every mutation
goes through a `git` CLI subprocess instead: `git add`, `git restore`,
`git apply --cached`, `git commit -F -`. That costs a process spawn per
mutation, and buys the guarantee that your hooks and your config keep working
exactly as they do on the command line. See
[ADR 0001](docs/adr/0001-hybrid-backend-and-undo.md).

**Merges are predicted before you choose.** Ravix runs `git merge-tree` ahead of
the join menu, so the strategy list tells you what the merge will actually do
instead of finding out afterwards.

**Undo is a first-class operation.** `u` reverses the last mutating action.
Commits are undone through the reflog (`git reset --soft HEAD@{1}`); destructive
working-tree operations are undone from a silent snapshot written as a blob into
the object database before the operation runs — no `git stash` entries, no
visible refs. The mapping from an action to its inverse is a pure function in
`src/undo.rs`, independent of IO, which is where the unit tests bite.

**Built to stay fluid under load.** Startup under 200 ms, and responsive at
32k commits and 2k branches.

## Requirements

- Rust 1.88 or newer (edition 2024, and `ratatui` sets the floor at 1.88)
- `git` on `PATH` — every mutation shells out to it
- A terminal with truecolor support

## Install

```sh
git clone https://github.com/lionelx87/ravix.git
cd ravix
cargo install --path .
```

The binary is called `rx`. Run it inside any git repository:

```sh
rx
```

To run without installing: `cargo run --release`.

## Keys

Press `?` inside Ravix for the full, context-sensitive list. The core:

| Key | Action |
| --- | --- |
| `j` / `k` | select next / previous commit |
| `gg` / `G` | jump to first / last |
| `Enter` | open the commit detail panel |
| `Space` | checkout commit (detached HEAD if no branch) |
| `f` / `p` / `P` | fetch / pull / push |
| `s` / `S` | stash changes / stash list |
| `>` / `<` | enter / exit a submodule |
| `u` | undo the last mutating action |
| `Ctrl+P` | command palette |
| `?` | help |
| `Esc` | collapse the panel, or back to the parent repo |

In the working-directory panel, `Space` stages or unstages the selected file or
hunk and `Tab` moves focus between files and hunks.

## Architecture

```
src/
  graph.rs      commit walk and graph layout
  git.rs        reads through git2
  mutate.rs     every write, through the git CLI
  undo.rs       pure action -> inverse-plan mapping
  join.rs       merge-tree prediction and strategy menu
  ui/           ratatui rendering
```

Around 11.7k lines of implementation and 9k lines of tests.

## Tests

```sh
cargo test
```

358 tests across 52 binaries. CI additionally enforces `cargo fmt --check` and
`cargo clippy --all-targets -D warnings`.

## Roadmap

Phases and the v2 wishlist — interactive visual rebase, visual `git bisect`,
blame, a multi-step undo timeline — live in [ROADMAP.md](ROADMAP.md).

## License

MIT. See [LICENSE](LICENSE).
