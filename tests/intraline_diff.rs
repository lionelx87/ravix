use ogma::enrich::{WordKind, WordSpan, word_diff};

fn equal(text: &str) -> WordSpan {
    WordSpan {
        kind: WordKind::Equal,
        text: text.to_string(),
    }
}

fn removed(text: &str) -> WordSpan {
    WordSpan {
        kind: WordKind::Removed,
        text: text.to_string(),
    }
}

fn added(text: &str) -> WordSpan {
    WordSpan {
        kind: WordKind::Added,
        text: text.to_string(),
    }
}

#[test]
fn a_single_word_substitution_marks_only_that_word() {
    let spans = word_diff("let x = 1;", "let y = 1;");

    assert_eq!(
        spans,
        vec![equal("let "), removed("x"), added("y"), equal(" = 1;")]
    );
}

#[test]
fn identical_lines_are_all_equal() {
    let spans = word_diff("same line", "same line");

    assert_eq!(spans, vec![equal("same line")]);
}

#[test]
fn an_appended_phrase_is_marked_added() {
    let spans = word_diff("foo", "foo bar");

    assert_eq!(spans, vec![equal("foo"), added(" bar")]);
}
