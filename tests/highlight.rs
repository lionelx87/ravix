use ogma::enrich::highlighter;

#[test]
fn highlighting_a_line_is_deterministic_across_calls() {
    let hl = highlighter();
    let syntax = hl.language("x.rs").expect("rust syntax available");

    let first = hl.highlight(syntax, "let value = compute();");
    let second = hl.highlight(syntax, "let value = compute();");

    assert_eq!(
        first, second,
        "a re-highlighted line matches the first result (cache must not corrupt output)"
    );
    assert!(
        first.len() > 1,
        "the line is tokenized into multiple coloured spans"
    );
}
