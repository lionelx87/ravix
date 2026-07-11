use ogma::slide::{SlidePanel, advance_panel};

#[test]
fn opening_starts_collapsed_and_animating_open() {
    let panel = SlidePanel::opening(vec!['a', 'b', 'c']);

    assert_eq!(panel.slide, 0.0, "starts fully collapsed");
    assert_eq!(panel.target, 1.0, "animates toward open");
    assert_eq!(panel.selected, 0, "selection starts at the top");
    assert_eq!(panel.focused(), Some(&'a'));
}

#[test]
fn move_selection_clamps_at_both_edges() {
    let mut panel = SlidePanel::opening(vec!['a', 'b', 'c']);

    panel.move_selection(1);
    assert_eq!(panel.selected, 1);

    panel.move_selection(-5);
    assert_eq!(panel.selected, 0, "cannot move above the top");

    panel.move_selection(9);
    assert_eq!(panel.selected, 2, "cannot move past the bottom");
    assert_eq!(panel.focused(), Some(&'c'));
}

#[test]
fn move_selection_is_a_no_op_on_an_empty_list() {
    let mut panel: SlidePanel<char> = SlidePanel::opening(vec![]);

    panel.move_selection(3);

    assert_eq!(panel.selected, 0);
    assert_eq!(panel.focused(), None);
}

#[test]
fn advance_slides_open_and_never_reports_dismissable_while_opening() {
    let mut panel = SlidePanel::opening(vec!['a']);

    panel.advance(0.25);

    assert_eq!(panel.slide, 0.25, "slides toward the open target");
    assert!(
        !panel.is_dismissed(),
        "an opening panel is never dismissable"
    );

    panel.advance(1.0);
    assert_eq!(panel.slide, 1.0, "clamps at the open target");
}

#[test]
fn close_then_advance_settles_shut_and_reports_dismissable() {
    let mut panel = SlidePanel::opening(vec!['a']);
    panel.advance(1.0);

    panel.close();
    assert_eq!(panel.target, 0.0, "close aims at collapsed");

    panel.advance(0.5);
    assert_eq!(panel.slide, 0.5);
    assert!(
        !panel.is_dismissed(),
        "still sliding shut, not yet dismissable"
    );

    panel.advance(0.5);
    assert_eq!(panel.slide, 0.0, "clamps at collapsed");
    assert!(
        panel.is_dismissed(),
        "fully collapsed and closing is dismissable"
    );
}

#[test]
fn advance_panel_keeps_an_opening_panel_and_drops_a_settled_closing_one() {
    let mut panel = Some(SlidePanel::opening(vec!['a']));

    let dropped = advance_panel(&mut panel, 0.5);
    assert!(!dropped, "an opening panel is not dropped");
    assert!(panel.is_some());

    panel.as_mut().unwrap().advance(1.0);
    panel.as_mut().unwrap().close();
    let dropped = advance_panel(&mut panel, 1.0);
    assert!(dropped, "a settled closing panel is dropped");
    assert!(panel.is_none(), "and the option is cleared");
}
