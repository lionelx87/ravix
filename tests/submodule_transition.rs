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
