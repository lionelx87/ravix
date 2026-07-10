use ogma::rebase::parse_progress;

#[test]
fn parses_the_current_step_and_total() {
    assert_eq!(parse_progress("2\n", "5\n"), Some((2, 5)));
}

#[test]
fn tolerates_surrounding_whitespace() {
    assert_eq!(parse_progress("  3 ", "  3 "), Some((3, 3)));
}

#[test]
fn rejects_nonsense_and_impossible_progress() {
    assert_eq!(parse_progress("x", "5"), None);
    assert_eq!(parse_progress("1", "0"), None);
    assert_eq!(parse_progress("6", "5"), None);
}
