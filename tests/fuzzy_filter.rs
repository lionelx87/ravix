use ogma::palette::fuzzy_filter;

const COMMANDS: &[&str] = &["Fetch", "Push", "Pull", "Stash changes", "Stash list"];

#[test]
fn an_empty_query_returns_every_command_in_order() {
    assert_eq!(fuzzy_filter("", COMMANDS), vec![0, 1, 2, 3, 4]);
}

#[test]
fn a_subsequence_query_filters_to_matches() {
    assert_eq!(fuzzy_filter("sl", COMMANDS), vec![4]);
}

#[test]
fn a_query_that_is_not_a_subsequence_matches_nothing() {
    assert_eq!(fuzzy_filter("zzz", COMMANDS), Vec::<usize>::new());
}

#[test]
fn a_contiguous_word_start_match_ranks_ahead_of_a_scattered_one() {
    assert_eq!(fuzzy_filter("pu", &["Popular ui", "Pull"]), vec![1, 0]);
}
