mod common;

use std::path::Path;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tempfile::TempDir;

use common::{draw, dump, git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn settle(app: &mut App) {
    for _ in 0..40 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
}

fn commit_on(dir: &Path, file: &str, message: &str) {
    std::fs::write(dir.join(file), message).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", message]);
}

fn review_repo(dir: &Path) {
    init_repo(dir);
    commit_on(dir, "README", "Root");
    git(dir, &["checkout", "-qb", "feature/x"]);
    commit_on(dir, "x.rs", "x one");
    git(dir, &["checkout", "-q", "main"]);
    commit_on(dir, "main.rs", "main two");
    git(dir, &["checkout", "-qb", "feature/y"]);
    commit_on(dir, "y.rs", "y work");
    git(dir, &["checkout", "-q", "feature/x"]);
    git(dir, &["merge", "-q", "--no-ff", "--no-edit", "main"]);
    git(dir, &["merge", "-q", "--no-ff", "--no-edit", "feature/y"]);
    commit_on(dir, "x2.rs", "x two");
    git(dir, &["checkout", "-q", "main"]);
}

fn wait_for(app: &mut App, ready: impl Fn(&str) -> bool) -> String {
    for _ in 0..200 {
        app.update(Action::Tick(Duration::from_millis(16)));
        let view = screen(app);
        if ready(&view) {
            return view;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    screen(app)
}

fn diffed(app: &mut App) -> String {
    wait_for(app, |view| !view.contains("loading the diff"))
}

fn screen(app: &mut App) -> String {
    dump(&draw(app, 120, 30))
}

fn open_review_from_panel(app: &mut App, input: &mut InputMap, downs: usize) {
    press(app, input, KeyCode::Char('b'));
    settle(app);
    for _ in 0..downs {
        press(app, input, KeyCode::Char('j'));
    }
    press(app, input, KeyCode::Char('l'));
    settle(app);
}

#[test]
fn reviewing_a_branch_shows_only_its_own_commits_against_the_auto_base() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_review_from_panel(&mut app, &mut input, 1);

    let view = screen(&mut app);
    assert!(
        view.contains("BRANCH REVIEW"),
        "the mode has its own band:\n{view}"
    );
    assert!(
        view.contains("feature/x vs main (auto)"),
        "branch and base are named:\n{view}"
    );
    assert!(
        view.contains("x one") && view.contains("x two"),
        "own commits are listed:\n{view}"
    );
    assert!(
        !view.contains("y work"),
        "commits merged in from siblings are left out:\n{view}"
    );
    assert!(
        !view.contains("main two"),
        "commits merged in from the base are left out:\n{view}"
    );
    assert!(
        view.contains("┈"),
        "skipped merges leave a faint marker:\n{view}"
    );
}

#[test]
fn moving_skips_merge_markers_and_the_diff_follows_the_selected_commit() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    let first = diffed(&mut app);
    assert!(
        first.contains("x2.rs") && first.contains("+ x two"),
        "the newest own commit's diff shows first:\n{first}"
    );

    press(&mut app, &mut input, KeyCode::Char('j'));
    let second = diffed(&mut app);
    assert!(
        second.contains("❯ ") && second.contains("+ x one"),
        "one step down lands on the next own commit, past the merges:\n{second}"
    );
    assert!(
        !second.contains("+ x two"),
        "the diff follows the selection:\n{second}"
    );
}

#[test]
fn the_band_summarises_only_the_own_commits_once_ready() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    let view = wait_for(&mut app, |view| view.contains("2 files"));
    assert!(
        view.contains("2 commits · 1 author · 2 files · +2 −0"),
        "totals count x.rs and x2.rs only, never y.rs or main.rs:\n{view}"
    );
}

#[test]
fn picking_a_file_narrows_the_commits_to_those_that_touched_it() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    wait_for(&mut app, |view| view.contains("2 files"));

    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Enter);
    let narrowed = diffed(&mut app);
    assert!(
        narrowed.contains("x.rs") && narrowed.contains("x2.rs"),
        "the files zone lists every touched file:\n{narrowed}"
    );
    assert!(
        narrowed.contains("x one"),
        "the commit touching x.rs stays:\n{narrowed}"
    );
    assert!(
        !narrowed.contains("x two"),
        "commits not touching x.rs go:\n{narrowed}"
    );
    assert!(
        narrowed.contains("+ x one"),
        "the diff shows x.rs in that commit:\n{narrowed}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let restored = diffed(&mut app);
    assert!(
        restored.contains("BRANCH REVIEW"),
        "Esc in Files only leaves the zone:\n{restored}"
    );
    assert!(
        restored.contains("x two") && restored.contains("x one"),
        "the full list comes back:\n{restored}"
    );
}

#[test]
fn a_file_filter_carried_back_to_the_commits_stays_visible_until_esc() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    wait_for(&mut app, |view| view.contains("2 files"));

    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Tab);
    let carried = screen(&mut app);
    assert!(
        carried.contains("Own commits 1 of 2 · x.rs"),
        "the commits title says the list is narrowed and by what:\n{carried}"
    );
    assert!(
        carried.contains("● x.rs"),
        "the filtering file stays marked in Files:\n{carried}"
    );
    assert!(
        carried.contains("Esc clear filter"),
        "the key bar says how to undo it:\n{carried}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let restored = screen(&mut app);
    assert!(
        restored.contains("Own commits 2 ") && restored.contains("┈"),
        "Esc brings back every own commit and the merge markers:\n{restored}"
    );
    assert!(
        !restored.contains("Esc clear filter"),
        "the key bar goes back to normal:\n{restored}"
    );
}

#[test]
fn tabbing_through_files_reaches_the_diff_without_filtering() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    wait_for(&mut app, |view| view.contains("2 files"));

    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Tab);
    let diff = screen(&mut app);
    assert!(
        diff.contains("Own commits 2 ") && diff.contains("x two") && diff.contains("┈"),
        "only Enter filters; passing through Files keeps every own commit:\n{diff}"
    );
}

#[test]
fn enter_on_the_filtering_file_lifts_the_filter() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    wait_for(&mut app, |view| view.contains("2 files"));

    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Enter);
    let lifted = screen(&mut app);
    assert!(
        lifted.contains("Own commits 2 ") && lifted.contains("x two"),
        "Enter again on the same file shows every own commit:\n{lifted}"
    );
}

fn mouse(app: &mut App, input: &mut InputMap, kind: MouseEventKind, row: u16) {
    let event = MouseEvent {
        kind,
        column: 30,
        row,
        modifiers: KeyModifiers::NONE,
    };
    let area = Rect::new(0, 0, 120, 29);
    if let Some(action) = input.on_mouse(event, area, app.input_context()) {
        app.update(action);
    }
}

#[test]
fn the_mouse_never_reaches_the_graph_hidden_under_the_review() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    press(&mut app, &mut input, KeyCode::Char('L'));
    type_text(&mut app, &mut input, "feature/x");
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let (selected, offset) = (app.selected(), app.offset());

    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Down(MouseButton::Left),
        5,
    );
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Up(MouseButton::Left),
        5,
    );
    for _ in 0..4 {
        mouse(&mut app, &mut input, MouseEventKind::ScrollDown, 5);
    }
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Down(MouseButton::Left),
        2,
    );
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Drag(MouseButton::Left),
        4,
    );
    mouse(
        &mut app,
        &mut input,
        MouseEventKind::Up(MouseButton::Left),
        4,
    );
    settle(&mut app);

    assert_eq!(
        (app.selected(), app.offset()),
        (selected, offset),
        "the graph selection and scroll stay put"
    );
    assert!(app.join_menu().is_none(), "no drop reaches the graph");
    assert_eq!(
        git(dir.path(), &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "main",
        "nothing is checked out"
    );
    assert!(
        screen(&mut app).contains("BRANCH REVIEW"),
        "the review stays open"
    );
}

#[test]
fn keys_under_the_open_help_do_not_reach_the_graph() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    git(dir.path(), &["checkout", "-q", "feature/x"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    press(&mut app, &mut input, KeyCode::Char('L'));
    settle(&mut app);
    let selected = app.selected();

    press(&mut app, &mut input, KeyCode::Char('?'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Esc);

    assert_eq!(
        app.selected(),
        selected,
        "the graph under the review never moves"
    );
    let view = screen(&mut app);
    assert!(
        view.contains("BRANCH REVIEW"),
        "Esc only closed the help:\n{view}"
    );
}

#[test]
fn a_branch_filter_left_in_the_panel_does_not_swallow_the_first_esc() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('/'));
    type_text(&mut app, &mut input, "feature/x");
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Char('l'));
    settle(&mut app);

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let view = screen(&mut app);
    assert!(
        !view.contains("BRANCH REVIEW"),
        "the first Esc leaves the review:\n{view}"
    );
    assert!(
        view.contains("/feature/x"),
        "the panel filter is untouched:\n{view}"
    );
}

#[test]
fn esc_from_the_diff_without_filters_leaves_the_review() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    wait_for(&mut app, |view| view.contains("2 files"));

    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);

    let view = screen(&mut app);
    assert!(
        !view.contains("BRANCH REVIEW"),
        "with nothing to clear, Esc leaves:\n{view}"
    );
}

#[test]
fn scrolling_over_the_base_picker_does_not_reach_the_graph() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    press(&mut app, &mut input, KeyCode::Char('b'));
    let offset = app.offset();

    for _ in 0..4 {
        mouse(&mut app, &mut input, MouseEventKind::ScrollDown, 5);
    }

    assert_eq!(
        app.offset(),
        offset,
        "the graph under the picker never scrolls"
    );
}

#[test]
fn a_refresh_that_rewrites_the_commits_drops_the_stale_summary_and_selects_the_nearest() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    wait_for(&mut app, |view| view.contains("2 files"));
    press(&mut app, &mut input, KeyCode::Char('j'));

    let root = git(dir.path(), &["rev-list", "--max-parents=0", "main"]);
    git(dir.path(), &["checkout", "-q", "feature/x"]);
    git(dir.path(), &["reset", "-q", "--hard", root.trim()]);
    commit_on(dir.path(), "x5.rs", "x five");
    commit_on(dir.path(), "x6.rs", "x six");
    git(dir.path(), &["checkout", "-q", "main"]);
    app.update(Action::Reload);

    let refreshed = screen(&mut app);
    assert!(
        !refreshed.contains("2 files"),
        "the old totals are not shown as current:\n{refreshed}"
    );
    assert!(
        refreshed.contains("summarizing"),
        "a placeholder stands in until the new totals land:\n{refreshed}"
    );

    let reselected = diffed(&mut app);
    assert!(
        reselected.contains("+ x five"),
        "the selected commit is gone, so the one now in its place is selected:\n{reselected}"
    );

    let settled = wait_for(&mut app, |view| view.contains("2 files ·"));
    assert!(
        settled.contains("2 commits · 1 author · 2 files · +2 −0"),
        "the fresh totals arrive:\n{settled}"
    );
}

#[test]
fn a_huge_commit_shows_a_capped_diff_with_what_was_left_out() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());
    commit_on(dir.path(), "README", "Root");
    git(dir.path(), &["checkout", "-qb", "feature/vendor"]);
    let body: String = (0..3_000).map(|line| format!("line {line}\n")).collect();
    for file in ["a.txt", "b.txt"] {
        std::fs::write(dir.path().join(file), &body).unwrap();
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "vendor everything"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_review_from_panel(&mut app, &mut input, 1);
    wait_for(&mut app, |view| view.contains("2 files"));
    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Tab);
    for _ in 0..3_000 {
        app.update(Action::PageDown);
    }

    let view = diffed(&mut app);
    assert!(
        view.contains("2000 more lines not shown"),
        "the cut is announced at the end of the diff:\n{view}"
    );
}

#[test]
fn the_selected_commit_stays_in_view_below_a_long_run_of_merges() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());
    commit_on(dir.path(), "README", "Root");
    git(dir.path(), &["checkout", "-qb", "feature/x"]);
    commit_on(dir.path(), "x.rs", "x one");
    for side in 0..30 {
        let name = format!("side/{side}");
        git(dir.path(), &["checkout", "-qb", &name, "main"]);
        commit_on(
            dir.path(),
            &format!("side{side}.rs"),
            &format!("side {side}"),
        );
        git(dir.path(), &["checkout", "-q", "feature/x"]);
        git(dir.path(), &["merge", "-q", "--no-ff", "--no-edit", &name]);
    }
    git(dir.path(), &["checkout", "-q", "main"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_review_from_panel(&mut app, &mut input, 1);

    let view = screen(&mut app);
    assert!(
        view.contains("feature/x vs main (auto)"),
        "the review is on feature/x:\n{view}"
    );
    assert!(
        view.contains("❯ ") && view.contains("x one"),
        "the list scrolls to the selected own commit:\n{view}"
    );
    assert!(
        view.contains("┈┈ 30 merges ┈┈") && !view.contains("Merge branch 'side/"),
        "a run of merges collapses into one counted marker:\n{view}"
    );
}

fn type_text(app: &mut App, input: &mut InputMap, text: &str) {
    for character in text.chars() {
        press(app, input, KeyCode::Char(character));
    }
}

#[test]
fn a_chosen_base_lasts_only_while_the_review_is_open() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    press(&mut app, &mut input, KeyCode::Char('b'));
    type_text(&mut app, &mut input, "feature/y");
    press(&mut app, &mut input, KeyCode::Enter);
    let chosen = screen(&mut app);
    assert!(
        chosen.contains("feature/x vs feature/y (chosen)"),
        "the band names the chosen base:\n{chosen}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    let panel = screen(&mut app);
    assert!(
        !panel.contains("BRANCH REVIEW"),
        "Esc leaves the mode:\n{panel}"
    );
    assert!(
        panel.contains(" Branches "),
        "back where the review was opened from:\n{panel}"
    );

    press(&mut app, &mut input, KeyCode::Char('l'));
    settle(&mut app);
    let reopened = screen(&mut app);
    assert!(
        reopened.contains("feature/x vs main (auto)"),
        "reopening goes back to the detected base:\n{reopened}"
    );
}

#[test]
fn reviewing_the_trunk_itself_asks_for_a_base_first() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('L'));
    let picker = screen(&mut app);
    assert!(
        picker.contains("Review main against"),
        "with no trunk to compare, the base picker opens:\n{picker}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    let graph = screen(&mut app);
    assert!(
        !graph.contains("Review main against") && !graph.contains("BRANCH REVIEW"),
        "cancelling leaves the mode unopened:\n{graph}"
    );

    press(&mut app, &mut input, KeyCode::Char('L'));
    type_text(&mut app, &mut input, "feature/x");
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let review = screen(&mut app);
    assert!(
        review.contains("main vs feature/x (chosen)"),
        "picking a base opens the review:\n{review}"
    );
    assert!(
        review.contains("main two"),
        "main's own commits against feature/x:\n{review}"
    );
}

#[test]
fn slash_filters_the_commits_by_message() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    press(&mut app, &mut input, KeyCode::Char('/'));
    type_text(&mut app, &mut input, "one");
    press(&mut app, &mut input, KeyCode::Enter);
    let filtered = diffed(&mut app);
    assert!(
        filtered.contains("x one"),
        "matching commits stay:\n{filtered}"
    );
    assert!(!filtered.contains("x two"), "the rest go:\n{filtered}");
    assert!(
        filtered.contains("+ x one"),
        "the selection moves onto a match:\n{filtered}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let cleared = diffed(&mut app);
    assert!(
        cleared.contains("BRANCH REVIEW") && cleared.contains("x two"),
        "Esc clears the filter first:\n{cleared}"
    );
}

#[test]
fn y_hands_the_short_hash_to_the_clipboard() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);
    let tip = git(dir.path(), &["rev-parse", "--short", "feature/x"])
        .trim()
        .to_string();

    press(&mut app, &mut input, KeyCode::Char('y'));

    assert_eq!(app.take_clipboard().as_deref(), Some(tip.as_str()));
    let view = screen(&mut app);
    assert!(
        view.contains("y/Y copy"),
        "the key bar stays after copying:\n{view}"
    );
    assert!(
        view.contains(&format!("Copied {tip}")),
        "a notice confirms the copy:\n{view}"
    );
}

#[test]
fn v_toggles_the_side_by_side_diff() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    press(&mut app, &mut input, KeyCode::Char('v'));

    let view = diffed(&mut app);
    assert!(
        view.contains("side-by-side · [v] unified"),
        "the diff pane switches mode:\n{view}"
    );
}

#[test]
fn the_review_follows_the_branch_live_and_keeps_the_selection() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    git(dir.path(), &["checkout", "-q", "feature/x"]);
    commit_on(dir.path(), "x3.rs", "x three");
    app.update(Action::Reload);

    let view = diffed(&mut app);
    assert!(
        view.contains("x three"),
        "a new own commit appears:\n{view}"
    );
    assert!(
        view.contains("+ x two"),
        "the selection stays on the same commit:\n{view}"
    );
}

#[test]
fn the_review_closes_when_its_branch_is_deleted() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    git(dir.path(), &["branch", "-D", "feature/x"]);
    app.update(Action::Reload);
    settle(&mut app);

    let view = screen(&mut app);
    assert!(!view.contains("BRANCH REVIEW"), "the mode closes:\n{view}");
    assert!(
        view.contains("feature/x is gone"),
        "a notice explains why:\n{view}"
    );
}

#[test]
fn the_mode_has_its_own_help_page() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_review_from_panel(&mut app, &mut input, 1);

    press(&mut app, &mut input, KeyCode::Char('?'));
    let help = screen(&mut app);
    assert!(
        help.contains("Branch review"),
        "the help names the mode:\n{help}"
    );
    assert!(
        help.contains("copy the commit's short hash"),
        "and lists its keys:\n{help}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let view = screen(&mut app);
    assert!(
        view.contains("BRANCH REVIEW") && !view.contains("copy the commit's short hash"),
        "Esc closes the help, not the mode:\n{view}"
    );
}

fn first_line(view: &str) -> &str {
    view.lines().next().unwrap_or_default()
}

#[test]
fn the_review_unfolds_from_a_line_and_folds_back_on_exit() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('j'));

    press(&mut app, &mut input, KeyCode::Char('l'));
    let opening = screen(&mut app);
    assert!(
        opening.contains("BRANCH REVIEW"),
        "the band shows from the first frame:\n{opening}"
    );
    assert!(
        !first_line(&opening).contains("BRANCH REVIEW"),
        "it starts folded mid-screen:\n{opening}"
    );
    assert!(app.is_animating(), "the unfold animates");

    settle(&mut app);
    let open = screen(&mut app);
    assert!(
        first_line(&open).contains("BRANCH REVIEW"),
        "fully unfolded it fills the screen:\n{open}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let closing = screen(&mut app);
    assert!(
        closing.contains("BRANCH REVIEW"),
        "it folds back before disappearing:\n{closing}"
    );
    settle(&mut app);
    let closed = screen(&mut app);
    assert!(
        !closed.contains("BRANCH REVIEW"),
        "then it is gone:\n{closed}"
    );
}

#[test]
fn a_branch_with_nothing_of_its_own_says_it_is_already_in_its_base() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());
    commit_on(dir.path(), "README", "Root");
    git(dir.path(), &["branch", "feature/done"]);
    commit_on(dir.path(), "main.rs", "main moves on");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_review_from_panel(&mut app, &mut input, 1);

    let view = screen(&mut app);
    assert!(
        view.contains("feature/done vs main (auto)"),
        "the review opens:\n{view}"
    );
    assert!(
        view.contains("No own commits — already in main"),
        "the empty list explains itself:\n{view}"
    );
}

#[test]
fn the_full_commit_message_sits_above_the_diff() {
    let dir = TempDir::new().unwrap();
    review_repo(dir.path());
    git(dir.path(), &["checkout", "-q", "feature/x"]);
    std::fs::write(dir.path().join("x4.rs"), "x four").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(
        dir.path(),
        &[
            "commit",
            "-qm",
            "x four",
            "-m",
            "Because the parser needed it.",
        ],
    );
    git(dir.path(), &["checkout", "-q", "main"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    open_review_from_panel(&mut app, &mut input, 1);

    let view = screen(&mut app);
    assert!(
        view.contains("Because the parser needed it."),
        "the body is readable in the mode:\n{view}"
    );
}
