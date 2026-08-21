use ravix::stash::{StashEntry, StashMessage, parse_stash_list};

fn line(selector: &str, time: &str, subject: &str, oid: &str) -> String {
    format!("{selector}\u{0}{time}\u{0}{subject}\u{0}{oid}\n")
}

#[test]
fn a_named_entry_carries_its_branch_time_and_message() {
    let text = line(
        "stash@{0}",
        "1787281861",
        "On main: fix status refresh race",
        "b90dd1c9c4c2989e17b69c0d609fad8ff0949f45",
    );

    let entries = parse_stash_list(&text);

    assert_eq!(
        entries,
        vec![StashEntry {
            index: 0,
            oid: "b90dd1c9c4c2989e17b69c0d609fad8ff0949f45".to_string(),
            branch: Some("main".to_string()),
            time: 1_787_281_861,
            message: StashMessage::Named("fix status refresh race".to_string()),
        }]
    );
}

#[test]
fn an_auto_entry_keeps_the_base_commit_it_was_taken_on() {
    let text = line(
        "stash@{1}",
        "1787281800",
        "WIP on feature/graph-perf: c6e62b8 fix: select the best match",
        "e6a02180351f850d820525c7fe6fae0b1ba9c086",
    );

    let entries = parse_stash_list(&text);

    assert_eq!(entries[0].branch.as_deref(), Some("feature/graph-perf"));
    assert_eq!(
        entries[0].message,
        StashMessage::Auto {
            base_id: "c6e62b8".to_string(),
        }
    );
    assert_eq!(entries[0].message.text(), "no message");
}

#[test]
fn an_entry_taken_on_a_detached_head_has_no_branch() {
    let text = line(
        "stash@{0}",
        "1787282005",
        "On (no branch): named one",
        "abc",
    );

    let entries = parse_stash_list(&text);

    assert_eq!(entries[0].branch, None);
    assert_eq!(entries[0].message.text(), "named one");
}

#[test]
fn a_message_holding_a_colon_is_not_split_further() {
    let text = line(
        "stash@{0}",
        "1787282005",
        "On main: spike: async reload",
        "abc",
    );

    let entries = parse_stash_list(&text);

    assert_eq!(entries[0].message.text(), "spike: async reload");
}

#[test]
fn malformed_lines_are_ignored() {
    let text = format!(
        "not a stash line\n{}",
        line("stash@{0}", "1787282005", "On main: keep me", "abc")
    );

    let entries = parse_stash_list(&text);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message.text(), "keep me");
}

#[test]
fn an_empty_list_has_no_entries() {
    assert_eq!(parse_stash_list(""), vec![]);
}
