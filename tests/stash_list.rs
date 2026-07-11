use ravix::stash::{StashEntry, parse_stash_list};

const LIST: &str = "\
stash@{0}: WIP on main: 1a2b3c4 add feature
stash@{1}: On feature: quick fix
";

#[test]
fn parses_each_stash_line_into_an_indexed_entry() {
    let entries = parse_stash_list(LIST);

    assert_eq!(
        entries,
        vec![
            StashEntry {
                index: 0,
                message: "WIP on main: 1a2b3c4 add feature".to_string(),
            },
            StashEntry {
                index: 1,
                message: "On feature: quick fix".to_string(),
            },
        ]
    );
}

#[test]
fn an_empty_list_has_no_entries() {
    assert_eq!(parse_stash_list(""), vec![]);
}

#[test]
fn malformed_lines_are_ignored() {
    let text = "not a stash line\nstash@{0}: WIP on main: keep me\n";

    let entries = parse_stash_list(text);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].index, 0);
    assert_eq!(entries[0].message, "WIP on main: keep me");
}
