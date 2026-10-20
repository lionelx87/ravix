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
        InputContext::PinSwitcher => HelpPage {
            title: "Pin switcher",
            bindings: PIN_SWITCHER,
        },
        InputContext::CopyChooser => HelpPage {
            title: "Copy branch",
            bindings: BRANCH_PICKER,
        },
        InputContext::BranchReview => HelpPage {
            title: "Branch review",
            bindings: BRANCH_REVIEW,
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
    ("b / B", "branches panel / pin switcher"),
    ("L", "review the current branch's own commits"),
    ("y / Y", "copy the short hash / a branch name"),
    ("F", "focus sets"),
    ("> / <", "submodules: enter / exit"),
    ("n", "new branch"),
    ("u", "undo last action"),
    ("Esc", "collapse the panel · back to the parent repo"),
    ("Ctrl+P", "command palette"),
];

const BRANCH: &[(&str, &str)] = &[
    ("Enter", "checkout the focused branch"),
    ("n", "new branch"),
    ("d", "delete branch"),
    ("M", "join into HEAD"),
    ("Space", "hide / show in the graph"),
    ("o", "solo (again to show all)"),
    ("O", "pins only (again to show all)"),
    ("p", "pin (survives solo, remembered)"),
    ("/", "fuzzy filter (↵ applies, keys act on matches)"),
    ("l", "review the branch's own commits"),
    ("Y", "copy the branch name"),
];

const PIN_SWITCHER: &[(&str, &str)] = &[
    ("j / k", "select next / previous pin"),
    ("Enter", "checkout the selected pin"),
    ("1-9", "checkout that numbered pin"),
    ("Y", "copy the selected pin's name"),
    ("B / Esc", "close"),
];

const BRANCH_PICKER: &[(&str, &str)] = &[
    ("j / k", "select next / previous branch"),
    ("Enter", "copy the selected branch"),
    ("1-9", "copy that numbered branch"),
    ("Y / Esc", "close"),
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
    ("c", "continue (stash: finish and keep or drop the entry)"),
    ("s", "skip (rebase)"),
    ("A", "abort (stash: discard the applied files)"),
];

const STASH: &[(&str, &str)] = &[
    ("j / k", "move inside the focused zone"),
    ("Tab / ⇧Tab", "focus the next zone, or the previous one"),
    ("Enter", "fullscreen"),
    ("p", "pop"),
    ("a", "apply"),
    ("d", "drop"),
    ("x", "restore only the focused file"),
    ("b", "take the entry out to a new branch"),
    ("v", "side-by-side diff"),
    ("/", "filter the list"),
    ("S / Esc", "close"),
    ("", "a conflicting pop or apply opens the resolver"),
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
    ("i", "initialize the submodule"),
    ("u", "update the submodule to the recorded commit"),
    ("> / Esc", "close (< or Esc in the graph exits it)"),
];

const BRANCH_REVIEW: &[(&str, &str)] = &[
    ("j / k", "move inside the focused zone"),
    ("Tab / ⇧Tab", "focus commits → files → diff"),
    (
        "Enter",
        "in files: narrow the commits to that file (again to lift)",
    ),
    ("v", "toggle side-by-side diff"),
    ("b", "choose another base (this review only)"),
    ("/", "filter commits by message or author"),
    ("y", "copy the commit's short hash"),
    ("Y", "copy the reviewed branch's name"),
    ("Esc", "clear filters, then leave the review"),
    ("L", "leave the review"),
];

const COMMIT_DIFF: &[(&str, &str)] = &[
    ("j / k", "select next / previous file"),
    ("v", "toggle side-by-side diff"),
    ("y", "copy the commit's short hash"),
    ("Esc", "collapse to the graph"),
];

const FALLBACK: &[(&str, &str)] = &[
    ("type", "enter text"),
    ("Enter", "confirm"),
    ("Esc", "cancel"),
];
