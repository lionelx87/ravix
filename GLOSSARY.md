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

### Navigation

**Pin switcher**:
A modal listing the pins by most recent checkout, used to check one out quickly.
_Avoid_: Favorites popup, quick switch, branch picker

**Copy chooser**:
A modal listing the branches on one commit, used to choose which branch name to copy.
_Avoid_: Branch picker, copy menu
