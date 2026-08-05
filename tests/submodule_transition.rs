mod common;

use std::time::Duration;

use common::{open_panel, press, submodule_fixture};
use crossterm::event::KeyCode;
use tempfile::TempDir;

use ravix::app::{Action, App, NavDirection};
use ravix::event::InputMap;

fn enter_submodule(app: &mut App, input: &mut InputMap) {
    open_panel(app, input);
    press(app, input, KeyCode::Enter);
}

#[test]
fn entering_a_submodule_plays_a_push_transition_that_ticks_to_completion() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    enter_submodule(&mut app, &mut input);

    assert!(app.breadcrumb().is_some(), "should now be inside the submodule");
    let transition = app.nav_transition().expect("push transition should start");
    assert_eq!(transition.direction, NavDirection::Push);
    assert!(app.is_animating(), "transition should drive the frame clock");

    app.update(Action::Tick(Duration::from_millis(500)));
    assert!(
        app.nav_transition().is_none(),
        "transition should settle after its duration"
    );
}

#[test]
fn exiting_a_submodule_plays_a_pop_transition_back_to_the_parent() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    enter_submodule(&mut app, &mut input);
    app.update(Action::Tick(Duration::from_millis(500)));

    press(&mut app, &mut input, KeyCode::Char('<'));

    assert!(app.breadcrumb().is_none(), "should be back in the parent");
    let transition = app.nav_transition().expect("pop transition should start");
    assert_eq!(transition.direction, NavDirection::Pop);

    app.update(Action::Tick(Duration::from_millis(500)));
    assert!(app.nav_transition().is_none());
}

#[test]
fn exiting_a_submodule_reopens_the_panel_focused_on_the_submodule() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    enter_submodule(&mut app, &mut input);
    app.update(Action::Tick(Duration::from_millis(500)));

    press(&mut app, &mut input, KeyCode::Char('<'));
    app.update(Action::Tick(Duration::from_millis(500)));

    assert!(app.breadcrumb().is_none(), "should be back in the parent");
    let panel = app
        .submodule_panel()
        .expect("panel should stay open after returning to the parent");
    let focused = panel.focused().expect("a submodule should be focused");
    assert_eq!(focused.path, "modules/sub");
}

#[test]
fn esc_returns_to_the_parent_repository() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    enter_submodule(&mut app, &mut input);
    app.update(Action::Tick(Duration::from_millis(500)));

    press(&mut app, &mut input, KeyCode::Esc);

    assert!(app.breadcrumb().is_none(), "should be back in the parent");
    let transition = app.nav_transition().expect("pop transition should start");
    assert_eq!(transition.direction, NavDirection::Pop);
}

#[test]
fn esc_closes_the_submodule_panel_before_leaving() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    enter_submodule(&mut app, &mut input);
    app.update(Action::Tick(Duration::from_millis(500)));
    open_panel(&mut app, &mut input);

    press(&mut app, &mut input, KeyCode::Esc);
    assert!(
        app.breadcrumb().is_some(),
        "the first Esc should only close the panel"
    );

    app.update(Action::Tick(Duration::from_millis(500)));
    press(&mut app, &mut input, KeyCode::Esc);
    assert!(app.breadcrumb().is_none(), "the second Esc should exit");
}

#[test]
fn esc_collapses_the_commit_panel_before_leaving() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    enter_submodule(&mut app, &mut input);
    app.update(Action::Tick(Duration::from_millis(500)));
    press(&mut app, &mut input, KeyCode::Enter);

    press(&mut app, &mut input, KeyCode::Esc);
    assert!(
        app.breadcrumb().is_some(),
        "the first Esc should only collapse the detail panel"
    );

    app.update(Action::Tick(Duration::from_millis(500)));
    press(&mut app, &mut input, KeyCode::Esc);
    assert!(app.breadcrumb().is_none(), "the second Esc should exit");
}

#[test]
fn esc_in_the_root_repository_does_nothing() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Esc);

    assert!(app.breadcrumb().is_none());
    assert!(
        app.nav_transition().is_none(),
        "there is no parent to navigate to"
    );
}
