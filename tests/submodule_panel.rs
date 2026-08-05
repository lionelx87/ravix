mod common;

use std::time::Duration;

use common::{draw, dump, git, open_panel, press, submodule_fixture};
use crossterm::event::KeyCode;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::event::InputMap;

#[test]
fn panel_rows_show_branch_sha_and_worktree_markers() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());
    let sub_dir = main.join("modules/sub");
    std::fs::write(sub_dir.join("file.txt"), "changed").unwrap();
    std::fs::write(sub_dir.join("new.txt"), "new").unwrap();
    let head = git(&sub_dir, &["rev-parse", "--short=7", "HEAD"])
        .trim()
        .to_string();

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);
    let screen = dump(&draw(&mut app, 120, 30));

    assert!(screen.contains("modules/sub"), "{screen}");
    assert!(screen.contains("main"), "branch missing:\n{screen}");
    assert!(
        screen.contains(&format!("@{head}")),
        "checked-out sha missing:\n{screen}"
    );
    assert!(screen.contains("±1"), "dirty marker missing:\n{screen}");
    assert!(screen.contains("?1"), "untracked marker missing:\n{screen}");
    assert!(screen.contains("1 synced"), "summary missing:\n{screen}");
}

#[test]
fn detail_card_explains_the_focused_submodule() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);
    let screen = dump(&draw(&mut app, 120, 30));

    assert!(screen.contains("../sub-origin"), "url missing:\n{screen}");
    assert!(screen.contains("recorded"), "recorded label missing:\n{screen}");
    assert!(screen.contains("in sync"), "sync verdict missing:\n{screen}");
    assert!(
        screen.contains("feat: first version"),
        "last commit missing:\n{screen}"
    );
    assert!(screen.contains("Enter enter"), "actions missing:\n{screen}");
}

#[test]
fn a_drifted_submodule_is_flagged_in_row_and_detail() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());
    let sub_dir = main.join("modules/sub");
    std::fs::write(sub_dir.join("file.txt"), "v2").unwrap();
    git(&sub_dir, &["add", "-A"]);
    git(&sub_dir, &["commit", "-qm", "feat: second version"]);

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);
    let screen = dump(&draw(&mut app, 120, 30));

    assert!(screen.contains('◆'), "drift glyph missing:\n{screen}");
    assert!(screen.contains("1 drifted"), "summary missing:\n{screen}");
    assert!(
        screen.contains("u update to recorded"),
        "drift action missing:\n{screen}"
    );
}

#[test]
fn inside_a_submodule_a_persistent_bar_shows_path_status_and_way_back() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());
    let sub_dir = main.join("modules/sub");
    std::fs::write(sub_dir.join("new.txt"), "new").unwrap();
    let head = git(&sub_dir, &["rev-parse", "--short=7", "HEAD"])
        .trim()
        .to_string();

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Enter);
    app.update(Action::Tick(Duration::from_secs(2)));

    let screen = dump(&draw(&mut app, 120, 30));
    let bar = screen.lines().next().unwrap();
    assert!(bar.contains("main › sub"), "path missing in bar:\n{screen}");
    assert!(bar.contains(&format!("@{head}")), "sha missing in bar:\n{screen}");
    assert!(bar.contains("● in sync"), "sync marker missing in bar:\n{screen}");
    assert!(bar.contains("?1"), "dirty marker missing in bar:\n{screen}");
    assert!(
        bar.contains("< / Esc back"),
        "return hint missing in bar:\n{screen}"
    );
}

#[test]
fn the_bar_flags_drift_against_the_parent_recorded_commit() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());
    let sub_dir = main.join("modules/sub");
    std::fs::write(sub_dir.join("file.txt"), "v2").unwrap();
    git(&sub_dir, &["add", "-A"]);
    git(&sub_dir, &["commit", "-qm", "feat: second version"]);

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);
    press(&mut app, &mut input, KeyCode::Enter);
    app.update(Action::Tick(Duration::from_secs(2)));

    let screen = dump(&draw(&mut app, 120, 30));
    let bar = screen.lines().next().unwrap();
    assert!(bar.contains("◆ drifted"), "drift marker missing in bar:\n{screen}");
}

#[test]
fn an_uninitialized_submodule_offers_init() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());
    git(&main, &["submodule", "deinit", "-f", "-q", "modules/sub"]);

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);
    let screen = dump(&draw(&mut app, 120, 30));

    assert!(screen.contains("(uninitialized)"), "{screen}");
    assert!(screen.contains("1 uninit"), "summary missing:\n{screen}");
    assert!(screen.contains("i init"), "init action missing:\n{screen}");
}
