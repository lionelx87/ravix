# 3. File history reads its log through the git CLI

- Status: Accepted
- Date: 2026-11-01
- Context: File history (#40)

## Context

ADR-0001 sends reads through git2. File history must list the commits that
changed one file while following it across renames, and keep a merge commit
only when it changed the file itself (a conflict resolution or a manual edit),
not when it merely carried changes over from a side branch. libgit2 has no
equivalent of `git log --follow` and no combined (`--cc`) diff, and the mode
must stream rows on repositories with tens of thousands of commits.

## Decision

File history reads its commit list with two `git` CLI subprocesses, both in
`src/git.rs`, run off the UI thread:

- `git log --follow -z --diff-merges=first-parent --numstat -M <tip> -- <path>`,
  read incrementally so rows appear as git finds them.
- `git diff-tree --cc -p <merge> -- <path>` for each merge in that log; the
  merge is listed only when the output is non-empty.

Everything else in the mode (file contents at a version, the diffs between two
versions, the files at HEAD, reachability) still goes through git2.

## Considered Options

- **Walk the history in git2 and detect renames per commit**: reimplements
  `--follow` heuristics, diverges from what `git log --follow` shows the user,
  and needs a full diff per commit with rename detection, which is slow on
  large repositories.
- **Keep every merge whose file differs from all parents**: also lists clean
  merges that combine both sides, which is noise the user did not want.

## Consequences

- File history needs a `git` executable on `PATH`, like every mutation already
  does.
- Its row list matches `git log --follow`, including its rename heuristics and
  limits (one path, renames detected by similarity).
