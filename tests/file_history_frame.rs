mod common;

use std::path::Path;
use std::time::Duration;

use crossterm::event::KeyCode;
use tempfile::TempDir;

use common::{draw, dump, git, init_repo, press};
use ravix::app::{Action, App};
use ravix::event::InputMap;

fn settle(app: &mut App) {
    for _ in 0..40 {
        app.update(Action::Tick(Duration::from_millis(16)));
    }
}

fn write(dir: &Path, file: &str, content: &str) {
    std::fs::write(dir.join(file), content).unwrap();
}

fn commit_as(dir: &Path, author: &str, message: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            &format!("user.name={author}"),
            "commit",
            "-qm",
            message,
        ],
    );
}

fn history_repo(dir: &Path) {
    init_repo(dir);
    std::fs::create_dir(dir.join("src")).unwrap();
    write(dir, "README", "readme\n");
    write(dir, "src/old.rs", "a\nb\nc\n");
    commit_as(dir, "Ana", "create the file");
    write(dir, "src/old.rs", "a\nB\nc\nd\n");
    commit_as(dir, "Lio", "second take");
    git(dir, &["mv", "src/old.rs", "src/new.rs"]);
    commit_as(dir, "Ana", "move it");
    git(dir, &["checkout", "-qb", "side"]);
    write(dir, "src/new.rs", "a\nB\nc\nd\nside\n");
    commit_as(dir, "Lio", "side tweak");
    git(dir, &["checkout", "-q", "main"]);
    write(dir, "src/new.rs", "top\na\nB\nc\nd\n");
    commit_as(dir, "Ana", "main tweak");
    git(dir, &["merge", "-q", "--no-edit", "side"]);
    git(dir, &["checkout", "-qb", "rival"]);
    write(dir, "src/new.rs", "rival\n");
    commit_as(dir, "Lio", "rival rewrite");
    git(dir, &["checkout", "-q", "main"]);
    write(dir, "src/new.rs", "ours\n");
    commit_as(dir, "Ana", "our rewrite");
    let _ = std::process::Command::new("git")
        .current_dir(dir)
        .args(["merge", "-q", "rival"])
        .output();
    write(dir, "src/new.rs", "settled\n");
    commit_as(dir, "Ana", "settle the rewrite");
    git(dir, &["checkout", "-qb", "later"]);
    write(dir, "src/new.rs", "settled\nlater\n");
    commit_as(dir, "Lio", "later line");
    git(dir, &["checkout", "-q", "main"]);
    write(dir, "README", "readme v2\n");
    commit_as(dir, "Ana", "touch readme");
    git(dir, &["merge", "-q", "--no-edit", "later"]);
}

fn screen(app: &mut App) -> String {
    dump(&draw(app, 140, 32))
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

fn loaded(app: &mut App) -> String {
    wait_for(app, |view| {
        !view.contains("searching") && !view.contains("loading the diff")
    })
}

fn type_text(app: &mut App, input: &mut InputMap, text: &str) {
    for character in text.chars() {
        press(app, input, KeyCode::Char(character));
    }
}

fn open_from_finder(app: &mut App, input: &mut InputMap, query: &str) -> String {
    press(app, input, KeyCode::Char('H'));
    type_text(app, input, query);
    press(app, input, KeyCode::Enter);
    settle(app);
    loaded(app)
}

#[test]
fn h_in_the_graph_opens_a_finder_over_head_files_and_choosing_one_opens_its_history() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('H'));
    let finder = screen(&mut app);
    assert!(
        finder.contains("src/new.rs") && finder.contains("README"),
        "the finder lists the files at HEAD:\n{finder}"
    );
    assert!(
        !finder.contains("src/old.rs"),
        "files gone from HEAD are not offered:\n{finder}"
    );

    type_text(&mut app, &mut input, "new");
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let view = loaded(&mut app);

    assert!(
        view.contains("FILE HISTORY") && view.contains("src/new.rs"),
        "the mode has its own band naming the file:\n{view}"
    );
    assert!(
        view.contains("settle the rewrite") && view.contains("Lio") && view.contains("Ana"),
        "each commit shows who made the change:\n{view}"
    );
    assert!(
        !view.contains("touch readme"),
        "commits that did not change the file are left out:\n{view}"
    );
}

#[test]
fn the_history_follows_renames_and_keeps_only_merges_that_changed_the_file_themselves() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    let view = open_from_finder(&mut app, &mut input, "new");

    assert!(
        view.contains("move it") && view.contains("↳ renamed from src/old.rs"),
        "the renaming commit is marked with the old path:\n{view}"
    );
    assert!(
        view.contains("create the file") && view.contains("second take"),
        "the history continues past the rename:\n{view}"
    );
    assert!(
        view.contains("side tweak")
            && view.contains("rival rewrite")
            && view.contains("later line"),
        "commits brought in by merges are listed:\n{view}"
    );
    assert!(
        view.contains("⇢ settle the rewrite") || view.contains("⇢ Merge branch 'rival'"),
        "a merge that resolved the file is listed and marked:\n{view}"
    );
    assert!(
        !view.contains("Merge branch 'side'") && !view.contains("Merge branch 'later'"),
        "merges that only carried changes over are left out:\n{view}"
    );
    assert!(
        view.contains("9 commits"),
        "the band counts the commits that changed the file:\n{view}"
    );
}

#[test]
fn uncommitted_changes_lead_the_history_and_compare_head_with_the_working_tree() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    write(dir.path(), "src/new.rs", "settled\nlater\nediting now\n");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    let view = open_from_finder(&mut app, &mut input, "new");

    let uncommitted = view.find("Uncommitted changes").unwrap_or(usize::MAX);
    let newest = view.find("later line").unwrap_or(0);
    assert!(
        uncommitted < newest,
        "the uncommitted row comes first:\n{view}"
    );
    assert!(
        view.contains("HEAD → working tree") && view.contains("+ editing now"),
        "it is selected and shows what changed since HEAD:\n{view}"
    );
}

fn short_of(dir: &Path, subject: &str) -> String {
    git(
        dir,
        &[
            "log",
            "--all",
            "--format=%h",
            &format!("--grep=^{subject}$"),
        ],
    )
    .trim()
    .to_string()
}

#[test]
fn an_anchor_shows_the_accumulated_change_between_two_versions() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    press(&mut app, &mut input, KeyCode::Char('G'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Char('g'));
    press(&mut app, &mut input, KeyCode::Char('g'));
    let view = loaded(&mut app);

    let oldest = short_of(dir.path(), "create the file");
    let newest = short_of(dir.path(), "later line");
    assert!(
        view.contains(&format!("{oldest} → {newest} · accumulated change")),
        "the diff heading names both versions, oldest first:\n{view}"
    );
    assert!(
        view.contains("- a") && view.contains("+ settled") && view.contains("+ later"),
        "the diff spans every change in between, across the rename:\n{view}"
    );
    assert!(
        view.contains(&format!("Commits · ◆ {oldest} create the file")),
        "the anchor is marked in the list:\n{view}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let cleared = loaded(&mut app);
    assert!(
        !cleared.contains("accumulated change") && cleared.contains("FILE HISTORY"),
        "Esc drops the anchor first and stays in the mode:\n{cleared}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    let graph = screen(&mut app);
    assert!(
        !graph.contains("FILE HISTORY"),
        "a second Esc leaves the mode:\n{graph}"
    );
}

#[test]
fn f_shows_the_whole_file_at_that_version_with_the_added_lines_marked() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    for _ in 0..7 {
        press(&mut app, &mut input, KeyCode::Char('j'));
    }
    press(&mut app, &mut input, KeyCode::Char('f'));
    let view = loaded(&mut app);

    assert!(
        view.contains("full file · 4 lines"),
        "the pane shows the file as it was after that commit:\n{view}"
    );
    assert!(
        view.contains("  a")
            && view.contains("+ B")
            && view.contains("+ d")
            && view.contains("  c"),
        "unchanged lines stay plain and the commit's lines are marked in place:\n{view}"
    );
    assert!(
        !view.contains("- b"),
        "removed lines are not part of the file:\n{view}"
    );

    press(&mut app, &mut input, KeyCode::Char('f'));
    let diff = loaded(&mut app);
    assert!(
        diff.contains("- b") && !diff.contains("full file ·"),
        "f again returns to the diff:\n{diff}"
    );
}

#[test]
fn h_on_a_file_of_a_commit_from_another_branch_starts_the_history_there() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    git(dir.path(), &["checkout", "-qb", "experiment"]);
    write(dir.path(), "src/new.rs", "settled\nlater\nexperiment\n");
    commit_as(dir.path(), "Lio", "try an experiment");
    git(dir.path(), &["checkout", "-q", "main"]);
    write(dir.path(), "src/new.rs", "settled\nlater\ndirty\n");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    let experiment = short_of(dir.path(), "try an experiment");
    for _ in 0..12 {
        let graph = screen(&mut app);
        let selected_line = graph.lines().find(|line| line.contains("❯")).unwrap_or("");
        if selected_line.contains("try an experiment") {
            break;
        }
        press(&mut app, &mut input, KeyCode::Char('j'));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('H'));
    settle(&mut app);
    let view = loaded(&mut app);

    assert!(
        view.contains(&format!("from experiment · {experiment}")),
        "the band says where the history starts:\n{view}"
    );
    assert!(
        view.contains(&format!("❯ {experiment}")),
        "the commit it was opened from is selected:\n{view}"
    );
    assert!(
        !view.contains("Uncommitted changes"),
        "uncommitted changes only belong to a history from HEAD:\n{view}"
    );
    assert!(
        view.contains("later line"),
        "older commits of the file follow:\n{view}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    let back = screen(&mut app);
    assert!(
        !back.contains("FILE HISTORY") && back.contains("try an experiment"),
        "Esc returns to the commit detail it came from:\n{back}"
    );
}

#[test]
fn enter_shows_the_selected_commit_in_the_graph_with_its_detail_open() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    for _ in 0..7 {
        press(&mut app, &mut input, KeyCode::Char('j'));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    let graph = screen(&mut app);

    assert!(!graph.contains("FILE HISTORY"), "the mode closes:\n{graph}");
    let selected = graph.lines().find(|line| line.contains("❯")).unwrap_or("");
    assert!(
        selected.contains("second take"),
        "that commit is selected in the graph:\n{graph}"
    );
    assert!(
        graph.contains("Author") && graph.contains("src/old.rs"),
        "its detail panel is open with every file it changed:\n{graph}"
    );
}

#[test]
fn h_in_the_working_directory_opens_the_focused_file_and_esc_returns_there() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    write(dir.path(), "src/new.rs", "settled\nlater\nediting now\n");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('g'));
    press(&mut app, &mut input, KeyCode::Char('g'));
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('H'));
    settle(&mut app);
    let view = loaded(&mut app);

    assert!(
        view.contains("FILE HISTORY") && view.contains("src/new.rs"),
        "the focused working file's history opens:\n{view}"
    );
    assert!(
        view.contains("Uncommitted changes") && view.contains("+ editing now"),
        "it starts on the uncommitted changes:\n{view}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    assert_eq!(
        app.input_context(),
        ravix::app::InputContext::Working,
        "Esc returns to the working directory"
    );
}

#[test]
fn h_on_a_file_in_branch_review_opens_its_history_from_the_reviewed_commit() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    git(dir.path(), &["checkout", "-qb", "experiment"]);
    write(dir.path(), "src/new.rs", "settled\nlater\nexperiment\n");
    commit_as(dir.path(), "Lio", "try an experiment");
    git(dir.path(), &["checkout", "-q", "main"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    press(&mut app, &mut input, KeyCode::Char('b'));
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('/'));
    type_text(&mut app, &mut input, "experiment");
    press(&mut app, &mut input, KeyCode::Enter);
    press(&mut app, &mut input, KeyCode::Char('l'));
    settle(&mut app);
    wait_for(&mut app, |view| !view.contains("summarizing"));
    press(&mut app, &mut input, KeyCode::Tab);
    press(&mut app, &mut input, KeyCode::Char('H'));
    settle(&mut app);
    let view = loaded(&mut app);

    let experiment = short_of(dir.path(), "try an experiment");
    assert!(
        view.contains(&format!("from experiment · {experiment}")) && view.contains("src/new.rs"),
        "the reviewed file's history starts at the reviewed commit:\n{view}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    settle(&mut app);
    let back = screen(&mut app);
    assert!(
        back.contains("BRANCH REVIEW") && !back.contains("FILE HISTORY"),
        "Esc returns to the branch review:\n{back}"
    );
}

#[test]
fn the_history_follows_edits_and_new_commits_live() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    write(dir.path(), "src/new.rs", "settled\nlater\nfresh edit\n");
    app.update(Action::Reload);
    let edited = loaded(&mut app);
    assert!(
        edited.contains("Uncommitted changes"),
        "an edit to the file shows up as uncommitted changes:\n{edited}"
    );

    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char('j'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Char('j'));
    commit_as(dir.path(), "Ana", "commit the fresh edit");
    app.update(Action::Reload);
    let committed = loaded(&mut app);
    let selected = committed
        .lines()
        .find(|line| line.contains("❯"))
        .unwrap_or("");
    assert!(
        selected.contains("rival rewrite"),
        "the selection is kept by commit:\n{committed}"
    );
    assert!(
        committed.contains("◆ ") && committed.contains("accumulated change"),
        "the anchor survives the reload:\n{committed}"
    );
    assert!(
        committed.contains("commit the fresh edit") && !committed.contains("Uncommitted changes"),
        "a new commit on HEAD joins the history:\n{committed}"
    );
    assert!(
        committed.contains("10 commits"),
        "the band recounts:\n{committed}"
    );
}

#[test]
fn slash_filters_the_commits_by_author_and_esc_lifts_the_filter_first() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    press(&mut app, &mut input, KeyCode::Char('/'));
    type_text(&mut app, &mut input, "Lio");
    press(&mut app, &mut input, KeyCode::Enter);
    let filtered = loaded(&mut app);
    assert!(
        filtered.contains("side tweak") && !filtered.contains("create the file"),
        "only that author's commits remain:\n{filtered}"
    );

    press(&mut app, &mut input, KeyCode::Esc);
    let lifted = loaded(&mut app);
    assert!(
        lifted.contains("create the file") && lifted.contains("FILE HISTORY"),
        "Esc lifts the filter and stays:\n{lifted}"
    );
}

#[test]
fn space_on_another_row_moves_the_anchor_and_the_list_title_always_names_it() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    press(&mut app, &mut input, KeyCode::Char('G'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    let anchored = loaded(&mut app);
    let oldest = short_of(dir.path(), "create the file");
    assert!(
        anchored.contains(&format!("Commits · ◆ {oldest} create the file")),
        "the list title names the anchor even when it scrolls away:\n{anchored}"
    );

    press(&mut app, &mut input, KeyCode::Char('k'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    let moved = loaded(&mut app);
    let second = short_of(dir.path(), "second take");
    assert!(
        moved.contains(&format!("Commits · ◆ {second} second take")),
        "Space on another row moves the anchor there:\n{moved}"
    );

    press(&mut app, &mut input, KeyCode::Char(' '));
    let dropped = loaded(&mut app);
    assert!(
        !dropped.contains("◆") && !dropped.contains("accumulated change"),
        "Space on the anchored row drops it:\n{dropped}"
    );
}

#[test]
fn an_anchored_comparison_survives_head_moving() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    write(dir.path(), "src/new.rs", "settled\nlater\nediting now\n");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    press(&mut app, &mut input, KeyCode::Char('G'));
    press(&mut app, &mut input, KeyCode::Char(' '));
    press(&mut app, &mut input, KeyCode::Char('g'));
    press(&mut app, &mut input, KeyCode::Char('g'));
    write(dir.path(), "README", "readme v3\n");
    git(dir.path(), &["add", "README"]);
    git(dir.path(), &["commit", "-qm", "unrelated readme"]);
    app.update(Action::Reload);
    let view = loaded(&mut app);

    assert!(
        view.contains("accumulated change")
            && view.contains("- a")
            && view.contains("+ editing now"),
        "the diff still spans from the anchor to the working tree:\n{view}"
    );
}

#[test]
fn the_full_file_opens_scrolled_to_the_first_added_line() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());
    let lines: Vec<String> = (1..=80).map(|n| format!("line {n}")).collect();
    write(dir.path(), "long.txt", &(lines.join("\n") + "\n"));
    commit_as(dir.path(), "Ana", "write the long file");
    let mut changed = lines.clone();
    changed[69] = "line 70 rewritten".to_string();
    write(dir.path(), "long.txt", &(changed.join("\n") + "\n"));
    commit_as(dir.path(), "Lio", "rewrite line seventy");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "long");

    press(&mut app, &mut input, KeyCode::Char('f'));
    let view = loaded(&mut app);

    assert!(
        view.contains("+ line 70 rewritten"),
        "the first added line is in view:\n{view}"
    );
}

#[test]
fn a_commit_in_the_middle_of_another_branch_names_that_branch_in_the_band() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    git(dir.path(), &["checkout", "-qb", "experiment"]);
    write(dir.path(), "src/new.rs", "settled\nlater\nexperiment\n");
    commit_as(dir.path(), "Lio", "try an experiment");
    write(dir.path(), "README", "experiment readme\n");
    commit_as(dir.path(), "Lio", "experiment readme");
    git(dir.path(), &["checkout", "-q", "main"]);
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();

    for _ in 0..12 {
        let graph = screen(&mut app);
        let selected_line = graph.lines().find(|line| line.contains("❯")).unwrap_or("");
        if selected_line.contains("try an experiment") {
            break;
        }
        press(&mut app, &mut input, KeyCode::Char('j'));
    }
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, &mut input, KeyCode::Char('H'));
    settle(&mut app);
    let view = loaded(&mut app);

    let experiment = short_of(dir.path(), "try an experiment");
    assert!(
        view.contains(&format!("from experiment · {experiment}")),
        "the branch holding the commit is named:\n{view}"
    );
}

#[test]
fn a_selected_commit_that_vanishes_hands_the_selection_to_the_nearest_row() {
    let dir = TempDir::new().unwrap();
    history_repo(dir.path());
    write(dir.path(), "src/new.rs", "settled\nlater\nediting now\n");
    let mut app = App::open(dir.path()).unwrap();
    let mut input = InputMap::default();
    open_from_finder(&mut app, &mut input, "new");

    press(&mut app, &mut input, KeyCode::Char('j'));
    git(dir.path(), &["update-ref", "refs/heads/main", "HEAD~1"]);
    app.update(Action::Reload);
    let view = loaded(&mut app);

    assert!(
        !view.contains("later line"),
        "the commit left the history:\n{view}"
    );
    let selected = view.lines().find(|line| line.contains("❯")).unwrap_or("");
    assert!(
        selected.contains("settle the rewrite"),
        "the selection lands on the row that took its place:\n{view}"
    );
}
