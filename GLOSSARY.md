# Ravix

A git TUI built around an animated commit graph, with contextual panels for branches, working directory, stashes and submodules.

## Language

### Branch visibility

**Pin**:
A branch the user has marked as one of theirs in a repo; it stays visible in the graph regardless of hiding or solo, and is remembered across sessions until unpinned or the branch disappears.
_Avoid_: Favorite, star, bookmark

**Solo**:
A visibility state where the graph shows only one chosen branch, plus every pin and HEAD.
_Avoid_: Isolate, filter

**Pins-only**:
A visibility state where the graph shows only the pins, their upstreams and HEAD.
_Avoid_: Favorites view, solo pins

**Focus set**:
A named, saved record of which branches are hidden, restorable later; it never includes pins.
_Avoid_: Workspace, preset, view

### Branch analysis

**Own commits**:
The commits created directly on a branch since it left its base, following only its first-parent line and excluding merge commits and everything brought in by merges.
_Avoid_: Branch commits, branch log, history

**Base**:
The branch whose first-parent line marks where a branch's own commits end, chosen automatically as the nearest trunk and changeable by the user.
_Avoid_: Parent branch, upstream, target

**Branch review**:
A read-only mode with its own identity, dedicated to inspecting one branch's own commits against its base.
_Avoid_: Branch log, branch lens, history view

### File analysis

**File history**:
A read-only mode with its own identity, dedicated to walking the commits that changed one file, following it across renames, each shown with its author and that file's change.
_Avoid_: File log, file timeline, blame

**Anchor**:
A version of the file marked in file history so the change shown is the accumulated one between it and the selected version.
_Avoid_: Pin, base, mark

### Cherry-picking

**Cherry-pick bench**:
A mode with its own identity for copying commits from one or more source branches onto a destination branch, leaving the sources untouched.
_Avoid_: Cherry-pick mode, commit transfer, move commits

**Pick**:
A commit marked on the cherry-pick bench to be copied onto the destination; picks persist while the source changes.
_Avoid_: Selection, basket, marked commit

**Destination**:
The local branch that receives the picks on the cherry-pick bench; never the same branch as the source.
_Avoid_: Target, onto branch

### Remotes

**Upstream**:
The remote branch a local branch is linked to; ahead/behind counts and a plain push are measured against it.
_Avoid_: Tracking branch, remote branch, base

**Mismatched upstream**:
An upstream whose branch name differs from the local branch's name, regardless of which remote it lives on.
_Avoid_: Foreign upstream, wrong upstream, borrowed upstream

**Push chooser**:
A modal shown when pushing a branch with a mismatched upstream, offering to push to its own name on the upstream's remote and relink, or to push into the mismatched upstream.
_Avoid_: Push menu, push picker

### Navigation

**Pin switcher**:
A modal listing the pins by most recent checkout, used to check one out quickly.
_Avoid_: Favorites popup, quick switch, branch picker

**Copy chooser**:
A modal listing the branches on one commit, used to choose which branch name to copy.
_Avoid_: Branch picker, copy menu
