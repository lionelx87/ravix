# 1. Hybrid git backend and the undo foundation

- Status: Accepted
- Date: 2026-07-09
- Context: Phase 2 (working directory), issues #3, #5, #7

## Context

Ogma reads a repository constantly (graph, status, diffs) and, from Phase 2
onward, mutates it (staging, discarding, committing). Reads and mutations have
different needs: reads want a fast in-process library; mutations must behave
exactly like the user's own `git` so that hooks and config are honored. The
tool also promises a safety net — any action can be undone with `u`.

## Decision

### Hybrid backend

- **Reads go through git2** (libgit2): repository discovery, the commit walk,
  ref/HEAD badges, working-tree status, and per-file diffs/hunks.
- **Every mutation goes through a `git` CLI subprocess** (`src/mutate.rs`):
  `git add`, `git restore --staged`, `git apply --cached [--reverse]`,
  `git restore`, `git commit -F -`, `git reset --soft`. This runs the user's
  hooks and respects their config, unlike direct index manipulation.

### Undo foundation

`u` provides **single-level** undo of the last mutating action (no redo; the
multi-step operation journal is a v2 goal):

- **Discard (file or hunk)** — before the destructive op, the affected file's
  pre-op content is written as a **blob into the object database** (git2) and
  its oid recorded; undo rewrites the working-tree file from that blob. Snapshots
  are silent: no `git stash` entries, no visible refs. A hunk discard snapshots
  the whole file.
- **Commit** — undone via the reflog with `git reset --soft HEAD@{1}`; the
  committed changes return to the index (staged) for re-editing.
- **Stage / unstage / stage-all / unstage-all** — undone by the inverse
  mutation (a hunk reuses its patch, reverse-applied).

The pure mapping from a recorded action to its inverse plan lives in
`src/undo.rs` (`invert`), independent of any IO, and is the unit-tested seam.

## Consequences

- **Single-level caveat.** Undo restores the recorded snapshot; if the affected
  file was edited again after the destructive op, `u` overwrites those later
  edits. Accepted for v1 — the v2 multi-step journal addresses it. A failed undo
  keeps the action in the slot so it can be retried.
- The subprocess boundary costs a process spawn per mutation; acceptable because
  mutations are user-initiated and infrequent relative to reads.
- Snapshots accumulate loose blobs in the object database; they are unreferenced
  and reclaimed by `git gc`.
