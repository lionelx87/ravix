use ravix::conflict::{Segment, Side, conflict_count, parse, resolve};

const CONFLICTED: &str = "\
fn main() {
<<<<<<< HEAD
    let value = 1;
=======
    let value = 2;
>>>>>>> feature
    println!(\"{value}\");
}
";

fn ctx(lines: &[&str]) -> Segment {
    Segment::Context(lines.iter().map(|line| line.to_string()).collect())
}

#[test]
fn parse_splits_context_and_conflict_blocks() {
    let segments = parse(CONFLICTED);

    assert_eq!(
        segments,
        vec![
            ctx(&["fn main() {"]),
            Segment::Conflict {
                ours: vec!["    let value = 1;".to_string()],
                theirs: vec!["    let value = 2;".to_string()],
            },
            ctx(&["    println!(\"{value}\");", "}"]),
        ]
    );
}

#[test]
fn resolve_taking_ours_keeps_the_head_side() {
    let segments = parse(CONFLICTED);

    let resolved = resolve(&segments, &[Side::Ours]);

    assert_eq!(
        resolved,
        "fn main() {\n    let value = 1;\n    println!(\"{value}\");\n}\n"
    );
}

#[test]
fn resolve_taking_theirs_keeps_the_incoming_side() {
    let segments = parse(CONFLICTED);

    let resolved = resolve(&segments, &[Side::Theirs]);

    assert_eq!(
        resolved,
        "fn main() {\n    let value = 2;\n    println!(\"{value}\");\n}\n"
    );
}

const TWO_BLOCKS: &str = "\
top
<<<<<<< HEAD
ours-a
=======
theirs-a
>>>>>>> feature
middle
<<<<<<< HEAD
ours-b
=======
theirs-b
>>>>>>> feature
bottom
";

#[test]
fn each_block_is_resolved_independently() {
    let segments = parse(TWO_BLOCKS);
    assert_eq!(conflict_count(&segments), 2);

    let resolved = resolve(&segments, &[Side::Ours, Side::Theirs]);

    assert_eq!(resolved, "top\nours-a\nmiddle\ntheirs-b\nbottom\n");
}
