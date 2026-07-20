mod common;

use std::path::Path;
use std::time::Duration;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::event::InputMap;

fn rail_column(app: &mut App) -> usize {
    let buffer = common::draw(app, 120, 12);
    let dump = common::dump(&buffer);
    dump.lines()
        .find_map(|line| line.chars().position(|symbol| symbol == '┊'))
        .unwrap()
}

fn commit_file(dir: &Path, index: usize) {
    std::fs::write(dir.join("file.txt"), format!("v{index}")).unwrap();
    common::git(dir, &["add", "-A"]);
    common::git(dir, &["commit", "-qm", &format!("commit {index}")]);
}

#[test]
fn rail_widens_when_load_more_reveals_a_wide_pill() {
    let dir = TempDir::new().unwrap();
    common::init_repo(dir.path());
    commit_file(dir.path(), 0);
    common::git(dir.path(), &["branch", "a-very-wide-branch-pill"]);
    for index in 1..90 {
        commit_file(dir.path(), index);
    }

    let mut app = App::open_with_load_page(dir.path(), 4).unwrap();
    let narrow = rail_column(&mut app);
    app.update(Action::SelectLast);
    let wide = rail_column(&mut app);
    assert!(
        wide > narrow,
        "rail did not widen after load_more: {narrow} -> {wide}"
    );
}

#[test]
fn rail_shrinks_when_the_widest_branch_is_hidden() {
    let dir = TempDir::new().unwrap();
    common::init_repo(dir.path());
    commit_file(dir.path(), 0);
    common::git(dir.path(), &["branch", "a-very-wide-branch-pill"]);
    commit_file(dir.path(), 1);

    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    let wide = rail_column(&mut app);

    app.update(Action::ToggleBranches);
    app.update(Action::Tick(Duration::from_secs(2)));
    common::press(&mut app, &mut input, KeyCode::Char('j'));
    common::press(&mut app, &mut input, KeyCode::Char(' '));
    app.update(Action::Dismiss);
    app.update(Action::Tick(Duration::from_secs(2)));

    let narrow = rail_column(&mut app);
    assert!(
        narrow < wide,
        "rail did not shrink after hiding the branch: {wide} -> {narrow}"
    );
}
