mod common;

use std::time::Duration;

use common::{git, open_panel, press, submodule_fixture};
use crossterm::event::KeyCode;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::event::InputMap;
use ravix::submodule::SyncState;

#[test]
fn update_moves_a_drifted_submodule_back_to_recorded_and_undo_restores_it() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());
    let sub_dir = main.join("modules/sub");
    let recorded = git(&main, &["rev-parse", ":modules/sub"]).trim().to_string();
    std::fs::write(sub_dir.join("file.txt"), "v2").unwrap();
    git(&sub_dir, &["add", "-A"]);
    git(&sub_dir, &["commit", "-qm", "feat: second version"]);
    let drifted_to = git(&sub_dir, &["rev-parse", "HEAD"]).trim().to_string();

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);

    press(&mut app, &mut input, KeyCode::Char('u'));
    let now = git(&sub_dir, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(now, recorded, "update should move HEAD to the recorded commit");
    let entry = app.submodule_panel().unwrap().focused().unwrap();
    assert_eq!(entry.sync, SyncState::Synced, "panel entry should refresh");

    press(&mut app, &mut input, KeyCode::Esc);
    app.update(Action::Tick(Duration::from_secs(2)));
    press(&mut app, &mut input, KeyCode::Char('u'));
    let restored = git(&sub_dir, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(restored, drifted_to, "undo should restore the drifted commit");
}

#[test]
fn init_checks_out_an_uninitialized_submodule() {
    let temp = TempDir::new().unwrap();
    let main = submodule_fixture(temp.path());
    git(&main, &["submodule", "deinit", "-f", "-q", "modules/sub"]);

    let mut app = App::open(&main).unwrap();
    let mut input = InputMap::default();
    open_panel(&mut app, &mut input);
    assert_eq!(
        app.submodule_panel().unwrap().focused().unwrap().sync,
        SyncState::Uninitialized
    );

    press(&mut app, &mut input, KeyCode::Char('i'));

    assert!(main.join("modules/sub/file.txt").exists(), "worktree restored");
    let entry = app.submodule_panel().unwrap().focused().unwrap();
    assert_eq!(entry.sync, SyncState::Synced, "panel entry should refresh");
}
