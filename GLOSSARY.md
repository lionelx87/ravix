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

### Navigation

**Pin switcher**:
A modal listing the pins by most recent checkout, used to check one out quickly.
_Avoid_: Favorites popup, quick switch, branch picker
