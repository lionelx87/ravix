#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use ravix::app::{Action, App};
use ravix::event::InputMap;
use ravix::ui::render;

pub const NOW: i64 = 100_000;

pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "test"]);
}

pub fn submodule_fixture(root: &Path) -> PathBuf {
    let origin = root.join("sub-origin");
    std::fs::create_dir(&origin).unwrap();
    init_repo(&origin);
    std::fs::write(origin.join("file.txt"), "v1").unwrap();
    git(&origin, &["add", "-A"]);
    git(&origin, &["commit", "-qm", "feat: first version"]);

    let main = root.join("main");
    std::fs::create_dir(&main).unwrap();
    init_repo(&main);
    git(&main, &["config", "protocol.file.allow", "always"]);
    std::fs::write(main.join("README.md"), "hi").unwrap();
    git(&main, &["add", "-A"]);
    git(&main, &["commit", "-qm", "init"]);
    git(
        &main,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            "../sub-origin",
            "modules/sub",
        ],
    );
    git(&main, &["commit", "-qm", "add submodule"]);
    main
}

pub fn press(app: &mut App, input: &mut InputMap, code: KeyCode) {
    let event = KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    };
    if let Some(action) = input.on_key(event, app.input_context()) {
        app.update(action);
    }
}

pub fn open_panel(app: &mut App, input: &mut InputMap) {
    press(app, input, KeyCode::Char('>'));
    app.update(Action::Tick(Duration::from_secs(2)));
}

pub fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app, NOW)).unwrap();
    terminal.backend().buffer().clone()
}

pub fn dump(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut out = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        out.push('\n');
    }
    out
}
