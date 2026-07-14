use crate::app::InputContext;

pub struct HelpPage {
    pub title: &'static str,
    pub bindings: &'static [(&'static str, &'static str)],
}

pub fn context_help(context: InputContext) -> HelpPage {
    match context {
        InputContext::Graph => HelpPage {
            title: "Graph",
            bindings: GRAPH,
        },
        InputContext::Branch => HelpPage {
            title: "Branches",
            bindings: BRANCH,
        },
        InputContext::Working => HelpPage {
            title: "Working diff",
            bindings: WORKING,
        },
        InputContext::Join => HelpPage {
            title: "Join",
            bindings: JOIN,
        },
        InputContext::Conflict => HelpPage {
            title: "Conflicts",
            bindings: CONFLICT,
        },
        InputContext::Stash => HelpPage {
            title: "Stashes",
            bindings: STASH,
        },
        InputContext::FocusSets => HelpPage {
            title: "Focus sets",
            bindings: FOCUS,
        },
        InputContext::Submodules => HelpPage {
            title: "Submodules",
            bindings: SUBMODULE,
        },
        InputContext::CommitDiff => HelpPage {
            title: "Commit diff",
            bindings: COMMIT_DIFF,
        },
        _ => HelpPage {
            title: "Keys",
            bindings: FALLBACK,
        },
    }
}

const GRAPH: &[(&str, &str)] = &[
    ("j / k", "select next / previous commit"),
    ("gg / G", "jump to first / last"),
    ("Enter", "open commit detail panel"),
    ("Space", "checkout commit (detached HEAD if no branch)"),
    ("M", "join: merge / cherry-pick / rebase"),
    ("f / p / P", "fetch / pull / push"),
    ("s / S", "stash changes / stash list"),
    ("b", "branches panel"),
    ("F", "focus sets"),
    ("> / <", "submodules: enter / exit"),
    ("n", "new branch"),
    ("u", "undo last action"),
    ("Ctrl+P", "command palette"),
];

const BRANCH: &[(&str, &str)] = &[
    ("Enter", "checkout the focused branch"),
    ("n", "new branch"),
    ("d", "delete branch"),
    ("M", "join into HEAD"),
    ("Space", "hide / show in the graph"),
    ("o", "solo (again to show all)"),
    ("p", "pin (survives solo)"),
    ("/", "fuzzy filter the list"),
];

const WORKING: &[(&str, &str)] = &[
    ("j / k", "select next / previous"),
    ("Space", "stage / unstage file or hunk"),
    ("a", "stage / unstage all"),
    ("Tab", "focus files / hunks"),
    ("v", "toggle side-by-side diff"),
    ("c", "commit staged changes"),
    ("d", "discard file or hunk"),
    ("s", "stash changes"),
    ("b", "branches panel"),
    ("Enter", "toggle fullscreen"),
];

const JOIN: &[(&str, &str)] = &[
    ("j / k", "select a join strategy"),
    ("Enter", "run the join"),
    ("M / Esc", "cancel"),
];

const CONFLICT: &[(&str, &str)] = &[
    ("j / k", "select a conflict block"),
    ("o / t", "take ours / theirs"),
    ("e", "edit in $EDITOR"),
    ("Tab", "next conflicted file"),
    ("c", "continue the operation"),
    ("s", "skip (rebase)"),
    ("A", "abort"),
];

const STASH: &[(&str, &str)] = &[
    ("j / k", "select a stash"),
    ("p", "pop"),
    ("a", "apply"),
    ("d", "drop"),
    ("S / Esc", "close"),
];

const FOCUS: &[(&str, &str)] = &[
    ("j / k", "select a focus set"),
    ("Enter", "activate the set"),
    ("n", "save current visibility"),
    ("d", "delete the set"),
    ("F / Esc", "close"),
];

const SUBMODULE: &[(&str, &str)] = &[
    ("j / k", "select a submodule"),
    ("Enter", "enter the submodule"),
    ("> / Esc", "close (use < in the graph to exit a submodule)"),
];

const COMMIT_DIFF: &[(&str, &str)] = &[
    ("j / k", "select next / previous file"),
    ("v", "toggle side-by-side diff"),
    ("Esc", "collapse to the graph"),
];

const FALLBACK: &[(&str, &str)] = &[
    ("type", "enter text"),
    ("Enter", "confirm"),
    ("Esc", "cancel"),
];
