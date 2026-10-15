mod common;

use std::path::Path;
use std::time::Duration;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use common::{draw, dump, git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn settle(app: &mut App) {
    for _ in 0..24 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
}

fn commit_on(dir: &Path, file: &str, message: &str) {
    std::fs::write(dir.join(file), message).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", message]);
}

fn branchy_repo(dir: &Path) {
    init_repo(dir);
    commit_on(dir, "README", "Root");
    for branch in ["alpha", "beta", "gamma"] {
        git(dir, &["checkout", "-qb", branch, "main"]);
        commit_on(dir, &format!("{branch}.rs"), &format!("{branch} work"));
    }
    git(dir, &["checkout", "-q", "main"]);
    commit_on(dir, "main.rs", "main work");
}

fn screen(app: &mut App) -> String {
    dump(&draw(app, 100, 24))
}

fn pin_in_panel(app: &mut App, input: &mut InputMap, downs: usize) {
    press(app, input, KeyCode::Char('b'));
    settle(app);
    for _ in 0..downs {
        press(app, input, KeyCode::Char('j'));
    }
    press(app, input, KeyCode::Char('p'));
    press(app, input, KeyCode::Esc);
    settle(app);
}

#[test]
fn pins_survive_a_relaunch() {
    let dir = TempDir::new().unwrap();
    branchy_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    pin_in_panel(&mut app, &mut input, 1);

    let mut relaunched = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    press(&mut relaunched, &mut input, KeyCode::Char('b'));
    settle(&mut relaunched);

    let panel = screen(&mut relaunched);
    assert!(
        panel.contains("★ alpha"),
        "the pin is read back from disk on relaunch:\n{panel}"
    );
}

#[test]
fn pins_only_narrows_the_graph_to_pins_and_head() {
    let dir = TempDir::new().unwrap();
    branchy_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    pin_in_panel(&mut app, &mut input, 1);

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('O'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let graph = screen(&mut app);
    assert!(graph.contains("alpha work"), "the pin stays:\n{graph}");
    assert!(graph.contains("main work"), "HEAD stays:\n{graph}");
    assert!(
        !graph.contains("beta work"),
        "unpinned branches go:\n{graph}"
    );
    assert!(
        !graph.contains("gamma work"),
        "unpinned branches go:\n{graph}"
    );
}

fn head_branch(dir: &Path) -> String {
    git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string()
}

#[test]
fn the_switcher_lists_pins_by_recency_and_a_digit_checks_one_out() {
    let dir = TempDir::new().unwrap();
    branchy_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    pin_in_panel(&mut app, &mut input, 0);
    pin_in_panel(&mut app, &mut input, 1);
    pin_in_panel(&mut app, &mut input, 3);

    press(&mut app, &mut input, KeyCode::Char('B'));
    settle(&mut app);
    let modal = screen(&mut app);
    assert!(
        modal.contains("1 gamma"),
        "the most recently checked-out pin besides HEAD is 1:\n{modal}"
    );
    assert!(
        modal.contains("2 alpha"),
        "older checkouts follow:\n{modal}"
    );
    assert!(
        !modal.contains("1 main") && !modal.contains("3 main"),
        "the HEAD pin carries no number:\n{modal}"
    );

    press(&mut app, &mut input, KeyCode::Char('1'));
    settle(&mut app);

    assert_eq!(head_branch(dir.path()), "gamma");
    let after = screen(&mut app);
    assert!(
        !after.contains("2 alpha"),
        "checking out closes the switcher:\n{after}"
    );
}

#[test]
fn deleting_a_pinned_branch_unpins_it_and_undo_brings_both_back() {
    let dir = TempDir::new().unwrap();
    branchy_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    pin_in_panel(&mut app, &mut input, 1);

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('d'));
    press(&mut app, &mut input, KeyCode::Char('y'));
    press(&mut app, &mut input, KeyCode::Char('y'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('B'));
    settle(&mut app);
    let switcher = screen(&mut app);
    assert!(
        !switcher.contains("alpha"),
        "a deleted branch is no longer a pin:\n{switcher}"
    );
    press(&mut app, &mut input, KeyCode::Esc);

    press(&mut app, &mut input, KeyCode::Char('u'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    let panel = screen(&mut app);
    assert!(
        panel.contains("★ alpha"),
        "undoing the deletion restores the branch and its pin:\n{panel}"
    );
}

#[test]
fn a_pin_whose_branch_vanished_outside_ravix_is_dropped() {
    let dir = TempDir::new().unwrap();
    branchy_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    pin_in_panel(&mut app, &mut input, 1);
    git(dir.path(), &["branch", "-qD", "alpha"]);

    let mut relaunched = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    press(&mut relaunched, &mut input, KeyCode::Char('B'));
    settle(&mut relaunched);

    let switcher = screen(&mut relaunched);
    assert!(
        switcher.contains("no pins yet"),
        "the vanished branch is no longer pinned:\n{switcher}"
    );
}

fn repo_with_remote_only_branch(root: &Path) -> std::path::PathBuf {
    let origin = root.join("origin.git");
    git(
        root,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    let work = root.join("work");
    std::fs::create_dir(&work).unwrap();
    branchy_repo(&work);
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&work, &["checkout", "-qb", "review", "main"]);
    commit_on(&work, "review.rs", "review work");
    git(&work, &["push", "-q", "origin", "main", "review"]);
    git(&work, &["checkout", "-q", "main"]);
    git(&work, &["branch", "-qD", "review"]);
    git(&work, &["fetch", "-q", "origin"]);
    work
}

#[test]
fn a_remote_pin_switches_to_its_local_branch_once_it_exists() {
    let root = TempDir::new().unwrap();
    let work = repo_with_remote_only_branch(root.path());
    std::fs::create_dir_all(work.join(".git/ravix")).unwrap();
    std::fs::write(work.join(".git/ravix/pins"), "main\norigin/review\n").unwrap();
    let mut app = App::open(&work).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('B'));
    press(&mut app, &mut input, KeyCode::Char('1'));
    settle(&mut app);
    assert_eq!(
        head_branch(&work),
        "review",
        "first pick creates the tracking branch"
    );

    press(&mut app, &mut input, KeyCode::Char('B'));
    press(&mut app, &mut input, KeyCode::Char('1'));
    settle(&mut app);
    assert_eq!(head_branch(&work), "main", "1 toggles back to main");

    press(&mut app, &mut input, KeyCode::Char('B'));
    let switcher = screen(&mut app);
    press(&mut app, &mut input, KeyCode::Char('1'));
    settle(&mut app);
    assert_eq!(
        head_branch(&work),
        "review",
        "picking the remote pin again checks out the existing local branch:\n{switcher}"
    );
}

#[test]
fn the_cursor_never_rests_on_the_unselectable_head_row() {
    let dir = TempDir::new().unwrap();
    branchy_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    pin_in_panel(&mut app, &mut input, 0);

    press(&mut app, &mut input, KeyCode::Char('B'));
    let modal = screen(&mut app);
    assert!(
        modal.contains("● main") && !modal.contains("❯ ● main"),
        "the lone HEAD pin is listed but not selected:\n{modal}"
    );

    press(&mut app, &mut input, KeyCode::Enter);
    assert_eq!(head_branch(dir.path()), "main");
}

#[test]
fn pins_only_without_pins_explains_itself_and_hides_nothing() {
    let dir = TempDir::new().unwrap();
    branchy_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('O'));
    let graph = screen(&mut app);

    assert!(
        graph.contains("No pins yet"),
        "a notice explains why:\n{graph}"
    );
    assert!(graph.contains("beta work"), "nothing is hidden:\n{graph}");
}

#[test]
fn pins_only_keeps_the_upstream_of_each_pin() {
    let root = TempDir::new().unwrap();
    let work = repo_with_remote_only_branch(root.path());
    git(&work, &["push", "-qu", "origin", "alpha"]);
    git(&work, &["checkout", "-q", "alpha"]);
    git(&work, &["reset", "-q", "--hard", "main~1"]);
    commit_on(&work, "alpha.rs", "alpha rewrite");
    git(&work, &["checkout", "-q", "main"]);
    std::fs::create_dir_all(work.join(".git/ravix")).unwrap();
    std::fs::write(work.join(".git/ravix/pins"), "alpha\n").unwrap();
    let mut app = App::open(&work).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('O'));
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let graph = screen(&mut app);
    assert!(graph.contains("alpha rewrite"), "the pin stays:\n{graph}");
    assert!(
        graph.contains("alpha work"),
        "its upstream origin/alpha stays:\n{graph}"
    );
    assert!(
        !graph.contains("beta work"),
        "unpinned branches go:\n{graph}"
    );
}
