use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use tempfile::TempDir;

use ravix::app::{Action, App, InputContext};
use ravix::event::InputMap;

fn commit(repo: &Repository, base: Option<Oid>, files: &[(&str, String)], message: &str) -> Oid {
    let signature = Signature::new("Dev", "dev@example.com", &Time::new(1000, 0)).unwrap();
    let base_tree = base.map(|oid| repo.find_commit(oid).unwrap().tree().unwrap());
    let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
    for (path, body) in files {
        let blob = repo.blob(body.as_bytes()).unwrap();
        builder.insert(*path, blob, 0o100644).unwrap();
    }
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let parents: Vec<_> = base
        .into_iter()
        .map(|oid| repo.find_commit(oid).unwrap())
        .collect();
    let refs: Vec<_> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &refs)
        .unwrap()
}

fn long(marker: &str) -> String {
    (0..80).map(|n| format!("{marker} line {n}\n")).collect()
}

fn two_file_repo(dir: &TempDir) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir.path(), &opts).unwrap();
    let root = commit(&repo, None, &[("seed.txt", "seed\n".into())], "seed");
    commit(
        &repo,
        Some(root),
        &[("a.txt", long("a")), ("b.txt", long("b"))],
        "two long files",
    );
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

fn enter_commit_fullscreen(app: &mut App) {
    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel);
}

#[test]
fn tab_focuses_the_diff_and_arrows_scroll_instead_of_changing_file() {
    let dir = TempDir::new().unwrap();
    two_file_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_commit_fullscreen(&mut app);
    assert!(!app.panel().unwrap().diff_focused, "starts on the file list");
    let file_before = app.panel().unwrap().file;

    app.update(Action::ToggleFocus);
    assert!(app.panel().unwrap().diff_focused, "Tab focuses the diff pane");

    app.update(Action::SelectNext);
    assert!(
        app.panel().unwrap().diff_scroll > 0,
        "with the diff focused, Down scrolls the diff"
    );
    assert_eq!(
        app.panel().unwrap().file,
        file_before,
        "the file cursor does not move while the diff is focused"
    );
}

#[test]
fn refocusing_the_file_list_lets_arrows_change_file_again() {
    let dir = TempDir::new().unwrap();
    two_file_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_commit_fullscreen(&mut app);
    app.update(Action::ToggleFocus); // -> diff
    app.update(Action::ToggleFocus); // -> files
    assert!(!app.panel().unwrap().diff_focused);

    app.update(Action::SelectNext);
    assert_eq!(
        app.panel().unwrap().file,
        1,
        "back on the file list, Down moves to the next file"
    );
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn shift_tab_is_wired_to_toggle_focus_in_the_diff_contexts() {
    let mut input = InputMap::default();
    assert_eq!(
        input.on_key(key(KeyCode::BackTab, KeyModifiers::SHIFT), InputContext::CommitDiff),
        Some(Action::ToggleFocus),
        "Shift-Tab toggles focus in the commit diff"
    );
    assert_eq!(
        input.on_key(key(KeyCode::BackTab, KeyModifiers::SHIFT), InputContext::Working),
        Some(Action::ToggleFocus),
        "Shift-Tab toggles focus in the working view"
    );
}
