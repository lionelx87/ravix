mod common;

use std::path::Path;
use std::time::Duration;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use common::{draw, dump, git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn commit_on(dir: &Path, file: &str, content: &str, message: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", message]);
}

fn bench_repo(dir: &Path) {
    init_repo(dir);
    commit_on(dir, "README", "root", "Root");
    git(dir, &["checkout", "-qb", "feature/login"]);
    commit_on(dir, "login.rs", "form\n", "initial login form");
    commit_on(dir, "token.rs", "refresh\n", "fix token refresh");
    commit_on(dir, "remember.rs", "remember\n", "add remember-me");
    commit_on(dir, "deps.txt", "v2\n", "bump deps");
    git(dir, &["checkout", "-q", "main"]);
    git(dir, &["checkout", "-qb", "release/2.3"]);
    commit_on(dir, "release.txt", "2.3.0\n", "release 2.3.0");
    commit_on(dir, "login.rs", "hotfix\n", "hotfix 212");
    let token = git(dir, &["rev-parse", "feature/login~2"]);
    git(dir, &["cherry-pick", token.trim()]);
    git(dir, &["checkout", "-q", "main"]);
    git(dir, &["checkout", "-qb", "feature/other"]);
    commit_on(dir, "other.txt", "other\n", "other work");
    git(dir, &["checkout", "-q", "main"]);
}

fn settle(app: &mut App) {
    for _ in 0..40 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
}

fn screen(app: &mut App) -> String {
    dump(&draw(app, 140, 30))
}

fn wait_for(app: &mut App, ready: impl Fn(&str) -> bool) -> String {
    for _ in 0..300 {
        app.update(Action::Tick(Duration::from_millis(16)));
        let view = screen(app);
        if ready(&view) {
            return view;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    screen(app)
}

fn select(app: &mut App, input: &mut InputMap, summary: &str) {
    for _ in 0..30 {
        if app
            .selected_commit()
            .is_some_and(|commit| commit.summary == summary)
        {
            return;
        }
        press(app, input, KeyCode::Char('j'));
    }
    panic!("no commit {summary} in the graph");
}

fn open_bench_on(app: &mut App, input: &mut InputMap, summary: &str) {
    select(app, input, summary);
    press(app, input, KeyCode::Char('C'));
    settle(app);
}

#[test]
fn pressing_c_on_a_commit_opens_the_bench_with_it_picked_onto_head() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_bench_on(&mut app, &mut input, "add remember-me");

    let view = screen(&mut app);
    assert!(
        view.contains("CHERRY-PICK BENCH"),
        "the mode has its own band:\n{view}"
    );
    assert!(
        view.contains("feature/login → main"),
        "source and destination are named:\n{view}"
    );
    assert!(
        view.contains("● ") && view.contains("◌ "),
        "the commit is picked and hovers over the destination as a ghost:\n{view}"
    );
    assert!(
        view.contains("1 pick from 1 branch"),
        "the footer counts the picks:\n{view}"
    );
}

fn choose(app: &mut App, input: &mut InputMap, key: char, name: &str) {
    press(app, input, KeyCode::Char(key));
    for character in name.chars() {
        press(app, input, KeyCode::Char(character));
    }
    press(app, input, KeyCode::Enter);
    settle(app);
}

#[test]
fn commits_the_destination_already_has_are_dimmed_and_cannot_be_picked() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_bench_on(&mut app, &mut input, "add remember-me");

    choose(&mut app, &mut input, 'B', "release/2.3");
    let view = wait_for(&mut app, |view| view.contains("already in destination"));

    assert!(
        view.contains("feature/login → release/2.3"),
        "the destination changed:\n{view}"
    );
    let token_row = view
        .lines()
        .find(|line| line.contains("fix token refresh") && line.contains("≡"))
        .unwrap_or_else(|| panic!("the cherry-picked commit is marked as equivalent:\n{view}"));
    assert!(token_row.contains("already in destination"));

    select_row(&mut app, &mut input, "fix token refresh");
    press(&mut app, &mut input, KeyCode::Char(' '));
    assert!(
        screen(&mut app).contains("1 pick from 1 branch"),
        "an equivalent commit can't be picked"
    );
}

fn select_row(app: &mut App, input: &mut InputMap, summary: &str) {
    for _ in 0..20 {
        let bench = app.cherry_pick_bench().unwrap();
        if bench.rows[bench.selected].commit.summary == summary {
            return;
        }
        press(app, input, KeyCode::Char('j'));
    }
    panic!("no row {summary} on the bench");
}

#[test]
fn a_clean_copy_writes_the_destination_without_touching_a_dirty_working_tree() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    std::fs::write(dir.path().join("README"), "work in progress").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_bench_on(&mut app, &mut input, "add remember-me");
    choose(&mut app, &mut input, 'B', "release/2.3");
    select_row(&mut app, &mut input, "bump deps");
    press(&mut app, &mut input, KeyCode::Char(' '));

    let predicted = wait_for(&mut app, |view| view.matches("✓ clean").count() == 2);
    assert_eq!(
        predicted.matches("✓ clean").count(),
        2,
        "every ghost carries its prediction:\n{predicted}"
    );

    press(&mut app, &mut input, KeyCode::Enter);
    let landed = wait_for(&mut app, |view| {
        view.matches("already in destination").count() >= 3
    });

    assert_eq!(
        git(dir.path(), &["log", "-2", "--format=%s", "release/2.3"]),
        "bump deps\nadd remember-me\n",
        "the picks land oldest first, so the newest ends on top"
    );
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%an", "release/2.3"]),
        "test\n",
        "the original author is kept"
    );
    assert_eq!(git(dir.path(), &["branch", "--show-current"]), "main\n");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("README")).unwrap(),
        "work in progress",
        "the working tree is never touched"
    );
    assert!(
        landed.contains("no picks"),
        "the picks are cleared once copied:\n{landed}"
    );
}

#[test]
fn picks_survive_a_source_change_so_several_branches_can_be_gathered() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_bench_on(&mut app, &mut input, "add remember-me");

    choose(&mut app, &mut input, 'b', "feature/other");
    select_row(&mut app, &mut input, "other work");
    press(&mut app, &mut input, KeyCode::Char(' '));
    let view = wait_for(&mut app, |view| view.matches("✓ clean").count() == 2);

    assert!(
        view.contains("feature/other → main"),
        "the source changed:\n{view}"
    );
    assert!(
        view.contains("2 picks from 2 branches"),
        "the earlier pick is kept:\n{view}"
    );
    assert!(
        view.contains("◌") && view.contains("add remember-me") && view.contains("other work"),
        "both picks hover over the destination:\n{view}"
    );
}

fn conflicting_bench(dir: &Path) -> (App, InputMap) {
    bench_repo(dir);
    let mut app = App::open(dir).unwrap();
    let mut input = InputMap::default();
    open_bench_on(&mut app, &mut input, "add remember-me");
    choose(&mut app, &mut input, 'B', "release/2.3");
    select_row(&mut app, &mut input, "initial login form");
    press(&mut app, &mut input, KeyCode::Char(' '));
    (app, input)
}

#[test]
fn a_predicted_conflict_checks_out_the_destination_and_returns_once_resolved() {
    let dir = TempDir::new().unwrap();
    let (mut app, mut input) = conflicting_bench(dir.path());

    let predicted = wait_for(&mut app, |view| view.contains("⚠ conflicts in 1 file"));
    assert!(
        predicted.contains("⚠ conflicts in 1 file") && predicted.contains("· pending"),
        "the conflicting pick is flagged and the next one can't be predicted:\n{predicted}"
    );

    press(&mut app, &mut input, KeyCode::Enter);
    let asked = screen(&mut app);
    assert!(
        asked.contains("Check out release/2.3 and resolve conflicts?"),
        "a conflicting copy asks before checking out:\n{asked}"
    );

    press(&mut app, &mut input, KeyCode::Char('y'));
    assert!(
        app.conflict_browser().is_some(),
        "the conflict browser opens on the destination"
    );
    assert_eq!(
        git(dir.path(), &["branch", "--show-current"]),
        "release/2.3\n"
    );

    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(app.conflict_browser().is_none(), "the sequence finished");
    assert_eq!(
        git(dir.path(), &["log", "-2", "--format=%s", "release/2.3"]),
        "add remember-me\ninitial login form\n",
        "the remaining picks are applied after the conflict"
    );
    assert_eq!(
        git(dir.path(), &["branch", "--show-current"]),
        "main\n",
        "Ravix returns to the original branch"
    );
}

#[test]
fn aborting_the_conflicting_copy_returns_to_the_original_branch_untouched() {
    let dir = TempDir::new().unwrap();
    let (mut app, mut input) = conflicting_bench(dir.path());
    let before = git(dir.path(), &["rev-parse", "release/2.3"]);
    wait_for(&mut app, |view| view.contains("⚠ conflicts in 1 file"));

    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Char('y'));
    press(&mut app, &mut input, KeyCode::Char('A'));

    assert_eq!(git(dir.path(), &["branch", "--show-current"]), "main\n");
    assert_eq!(git(dir.path(), &["rev-parse", "release/2.3"]), before);
}

fn copy_bump_deps_onto_release(app: &mut App, input: &mut InputMap) {
    open_bench_on(app, input, "bump deps");
    choose(app, input, 'B', "release/2.3");
    wait_for(app, |view| view.contains("✓ clean"));
    press(app, input, KeyCode::Enter);
    press(app, input, KeyCode::Esc);
    settle(app);
}

#[test]
fn undo_restores_the_destination_tip_after_a_copy() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let before = git(dir.path(), &["rev-parse", "release/2.3"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    copy_bump_deps_onto_release(&mut app, &mut input);
    assert_ne!(git(dir.path(), &["rev-parse", "release/2.3"]), before);

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert_eq!(git(dir.path(), &["rev-parse", "release/2.3"]), before);
    assert!(screen(&mut app).contains("Undid copy onto release/2.3"));
}

#[test]
fn undo_refuses_when_the_destination_moved_since_the_copy() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    copy_bump_deps_onto_release(&mut app, &mut input);
    git(
        dir.path(),
        &["branch", "-f", "release/2.3", "release/2.3~2"],
    );
    app.update(Action::Reload);
    let moved = git(dir.path(), &["rev-parse", "release/2.3"]);

    press(&mut app, &mut input, KeyCode::Char('u'));

    assert_eq!(git(dir.path(), &["rev-parse", "release/2.3"]), moved);
    assert!(
        screen(&mut app).contains("release/2.3 moved since the copy; not undone"),
        "the user is told why nothing happened"
    );
}

#[test]
fn a_commit_that_is_no_branch_s_own_opens_the_source_picker() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_bench_on(&mut app, &mut input, "Root");

    let view = screen(&mut app);
    assert!(
        view.contains("Cherry-pick from…"),
        "the source is asked for:\n{view}"
    );
}

#[test]
fn copying_from_the_checked_out_branch_asks_for_the_destination() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    git(dir.path(), &["checkout", "-q", "feature/login"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_bench_on(&mut app, &mut input, "bump deps");

    let view = screen(&mut app);
    assert!(
        view.contains("Cherry-pick onto…"),
        "the destination is asked for:\n{view}"
    );
    let picker = app.bench_picker().expect("destination picker");
    assert!(
        !picker.rows().contains(&"feature/login"),
        "the source is never offered as destination"
    );
    assert!(
        !picker.rows().iter().any(|name| name.starts_with("origin/")),
        "only local branches receive picks"
    );

    choose_in_picker(&mut app, &mut input, "release/2.3");
    let opened = screen(&mut app);
    assert!(
        opened.contains("feature/login → release/2.3") && opened.contains("1 pick from 1 branch"),
        "choosing the destination opens the bench with the commit picked:\n{opened}"
    );
}

fn choose_in_picker(app: &mut App, input: &mut InputMap, name: &str) {
    for character in name.chars() {
        press(app, input, KeyCode::Char(character));
    }
    press(app, input, KeyCode::Enter);
    settle(app);
}

#[test]
fn the_join_menu_no_longer_offers_cherry_pick() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    select(&mut app, &mut input, "bump deps");

    press(&mut app, &mut input, KeyCode::Char('M'));
    settle(&mut app);

    let view = screen(&mut app);
    assert!(
        view.contains("Merge commit"),
        "the join menu is open:\n{view}"
    );
    assert!(
        !view.contains("Cherry-pick"),
        "cherry-pick lives on the bench now:\n{view}"
    );
}

#[test]
fn joining_a_commit_without_a_branch_points_to_the_bench() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    select(&mut app, &mut input, "initial login form");

    press(&mut app, &mut input, KeyCode::Char('M'));
    settle(&mut app);

    assert!(app.join_menu().is_none(), "no join menu opens");
    assert!(screen(&mut app).contains("Use C to cherry-pick a commit"));
}

#[test]
fn copying_onto_the_checked_out_branch_cherry_picks_onto_head() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_bench_on(&mut app, &mut input, "bump deps");
    wait_for(&mut app, |view| view.contains("✓ clean"));

    press(&mut app, &mut input, KeyCode::Enter);

    let head = app.head_commit().expect("HEAD commit");
    assert_eq!(head.summary, "bump deps");
    assert_eq!(
        head.parents.len(),
        1,
        "a cherry-pick is a single-parent commit"
    );
}

#[test]
fn a_conflict_onto_head_resolves_through_the_browser() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    git(dir.path(), &["checkout", "-q", "release/2.3"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_bench_on(&mut app, &mut input, "initial login form");
    wait_for(&mut app, |view| view.contains("⚠ conflicts in 1 file"));

    press(&mut app, &mut input, KeyCode::Enter);

    assert!(
        app.conflict_browser().is_some(),
        "no checkout is needed, the browser opens right away"
    );
    let browser = screen(&mut app);
    assert!(
        browser.contains("cherry-pick in progress"),
        "the browser shows a cherry-pick in progress:\n{browser}"
    );
    press(&mut app, &mut input, KeyCode::Char('t'));
    press(&mut app, &mut input, KeyCode::Char('c'));
    press(&mut app, &mut input, KeyCode::Enter);

    assert!(app.conflict_browser().is_none());
    assert_eq!(
        app.head_commit().expect("HEAD commit").summary,
        "initial login form"
    );
}

#[test]
fn copied_picks_fly_in_and_solidify_with_their_new_hashes() {
    let dir = TempDir::new().unwrap();
    bench_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_bench_on(&mut app, &mut input, "bump deps");
    choose(&mut app, &mut input, 'B', "release/2.3");
    wait_for(&mut app, |view| view.contains("✓ clean"));

    press(&mut app, &mut input, KeyCode::Enter);
    let written = git(dir.path(), &["rev-parse", "--short=7", "release/2.3"]);
    let written = written.trim();
    let row = |view: &str| {
        view.lines()
            .find(|line| line.contains(written))
            .map(str::to_string)
            .unwrap_or_default()
    };

    let flying = screen(&mut app);
    assert!(
        row(&flying).contains("➜"),
        "the new commit is still flying in:\n{flying}"
    );

    app.update(Action::Tick(Duration::from_millis(200)));
    let landed = screen(&mut app);
    assert!(
        row(&landed).contains("● ") && !row(&landed).contains("➜"),
        "the ghost solidifies with its new hash:\n{landed}"
    );

    settle(&mut app);
    let calm = screen(&mut app);
    assert!(
        row(&calm).contains("◉ "),
        "once landed it is the destination's ordinary tip:\n{calm}"
    );
}
