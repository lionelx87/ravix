use ravix::pins::{parse, serialize};

#[test]
fn pins_round_trip_through_text_keeping_pin_order() {
    let pins = vec![
        "zeta".to_string(),
        "feature/login".to_string(),
        "origin/release/v1".to_string(),
    ];

    assert_eq!(parse(&serialize(&pins)), pins);
}

#[test]
fn parse_skips_blank_lines() {
    assert_eq!(parse("\nmain\n\n  \nfeature\n"), vec!["main", "feature"]);
}
