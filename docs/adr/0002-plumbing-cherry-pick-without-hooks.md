# 2. Cherry-pick onto a non-HEAD destination through plumbing, without hooks

- Status: Accepted
- Date: 2026-10-28
- Context: Cherry-pick bench

## Context

The cherry-pick bench copies picks onto any local destination branch, not only
HEAD, and must not touch the working tree when it does so. ADR-0001 runs every
mutation through the `git` CLI so that hooks and config are honored, but
`git cherry-pick` only works on the checked-out branch.

## Decision

When the destination is not HEAD and every pick is predicted clean, Ravix
replays the picks with git plumbing, still as `git` CLI subprocesses:
`git merge-tree --write-tree --merge-base <parent> <destination> <pick>` for
each pick in order, `git commit-tree` with the pick's author and original
message, and a final `git update-ref` of the destination guarded by its old
tip. No commit hooks run on this path.

When the destination is HEAD, or any pick is predicted to conflict (after the
user confirms a checkout of the destination), Ravix uses a real
`git cherry-pick`, which runs hooks as usual.

## Considered Options

- **Temporary worktree** (`git worktree add`, `git cherry-pick`, remove): fully
  faithful to git and hooks, but checks out the whole tree, which costs
  seconds on large repositories and breaks the bench's instant feel.
- **Checkout, pick, checkout back**: touches the working tree and fails when it
  is dirty, which is exactly what the bench avoids.

## Consequences

- Commit hooks (`prepare-commit-msg`, `commit-msg`, `post-commit`) are skipped
  for clean copies onto a non-HEAD destination. This is a deliberate exception
  to the "hooks respected" pillar; do not "fix" it by switching to
  `git cherry-pick` without revisiting this record.
- Undo restores the destination ref to its previous tip only if the ref still
  points at the tip Ravix wrote.
