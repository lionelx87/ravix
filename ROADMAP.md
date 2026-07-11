# Ravix Roadmap

Ravix (binary `rx`) — a spectacular, animated git TUI. Personal daily driver.

## Design pillars (agreed 2026-07-09)

- Rust + Ratatui, pure cell-based art (truecolor, unicode curves, braille), no pixel protocols
- Graph-centric layout: the graph is home; contextual animated panels; fullscreen views on demand
- Keyboard-first (lazygit-style mnemonics, hjkl, `?` help, Ctrl+P command palette) + full mouse incl. branch drag & drop
- Hybrid backend: git2 for reads, git CLI subprocess for every mutation (hooks and config respected)
- Merge prediction via `git merge-tree` before showing the join strategy menu; ghost topology preview
- Universal undo (`u`): reflog restore + silent worktree snapshots before destructive ops
- Functional animation (100–200ms, easing) + punctual celebrations; single intensity dial, can be off
- Default visibility: local branches + upstreams; branch panel with fuzzy toggle/solo/pins; focus mode with saved sets
- Performance: <200ms startup, fluid at 32k commits / 2k branches

## v1 phases

| Phase | Deliverable | Spec | Status |
|---|---|---|---|
| 1 | Walking skeleton: open repo, render animated navigable graph, detail panel, help, auto-refresh | #1 | ✅ Done |
| 2 | Working directory: status row, staging/unstage/discard per file and hunk, enriched diffs (syntect, intra-line), commit flow, snapshots + undo foundation | #3, #5, #7 | ✅ Done |
| 3 | Branch operations: checkout, branch create/delete, join menu with merge-tree prediction, merge/rebase/cherry-pick, conflict browser, drag & drop | #8, #10, #11, #14, #15 | ✅ Done |
| 4 | Remotes & flow: fetch/pull/push, upstream tracking in graph, stash, command palette, celebrations | #16, #18, #19, #21, #22 | ✅ Done |
| 5 | Submodules (lazygit-style enter/exit with breadcrumb), focus mode, branch panel polish, side-by-side fullscreen diff | #20, #23, #2, #24, #25, #26, #13 | ✅ Done |

## v2 wishlist (do not lose)

- Interactive visual rebase (reorder/squash/edit by dragging commits)
- Visual `git bisect` (good/bad marking on the graph, animated binary search)
- Blame view
- Commit search
- Operation journal with multi-step undo timeline
- GitHub PRs via `gh`
