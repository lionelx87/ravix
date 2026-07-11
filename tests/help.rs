use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ravix::app::InputContext;
use ravix::event::InputMap;
use ravix::help::{HelpPage, context_help};

fn key_codes(label: &str) -> Vec<KeyCode> {
    label
        .split(" / ")
        .filter_map(|token| match token.trim() {
            "Enter" => Some(KeyCode::Enter),
            "Esc" => Some(KeyCode::Esc),
            "Tab" => Some(KeyCode::Tab),
            single if single.chars().count() == 1 => Some(KeyCode::Char(single.chars().next()?)),
            _ => None,
        })
        .collect()
}

fn describes(page: &HelpPage, needle: &str) -> bool {
    page.bindings
        .iter()
        .any(|(_, description)| description.contains(needle))
}

#[test]
fn each_navigation_context_is_titled_and_scoped_to_its_keys() {
    let graph = context_help(InputContext::Graph);
    let branch = context_help(InputContext::Branch);

    assert_eq!(graph.title, "Graph");
    assert_eq!(branch.title, "Branches");

    assert!(
        describes(&branch, "solo"),
        "the branch page lists the visibility keys"
    );
    assert!(
        !describes(&graph, "solo"),
        "the graph page does not carry branch-panel-only keys"
    );
}

#[test]
fn navigation_contexts_are_curated_and_inputs_fall_back() {
    let subs = context_help(InputContext::Submodules);
    assert_eq!(subs.title, "Submodules");
    assert!(
        describes(&subs, "enter"),
        "the submodules page documents entering a submodule"
    );

    for (context, title) in [
        (InputContext::Working, "Working diff"),
        (InputContext::Join, "Join"),
        (InputContext::Conflict, "Conflicts"),
        (InputContext::Stash, "Stashes"),
        (InputContext::FocusSets, "Focus sets"),
    ] {
        assert_eq!(context_help(context).title, title);
    }

    assert_eq!(
        context_help(InputContext::Commit).title,
        "Keys",
        "a text-input context falls back to the generic page"
    );
}

#[test]
fn every_panel_help_key_is_actually_bound_by_its_handler() {
    let contexts = [
        InputContext::Branch,
        InputContext::Stash,
        InputContext::FocusSets,
        InputContext::Submodules,
        InputContext::Join,
        InputContext::Conflict,
        InputContext::CommitDiff,
    ];
    for context in contexts {
        for (keys, description) in context_help(context).bindings {
            for code in key_codes(keys) {
                let event = KeyEvent {
                    code,
                    modifiers: KeyModifiers::NONE,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::NONE,
                };
                assert!(
                    InputMap::default().on_key(event, context).is_some(),
                    "{context:?} help lists '{keys}' ({description}) but its handler does not bind {code:?}"
                );
            }
        }
    }
}
