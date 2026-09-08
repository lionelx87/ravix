#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

pub fn git_stdin(dir: &Path, args: &[&str], input: &str) -> String {
    let mut child = Command::new("git")
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
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
    configure_identity(dir);
}

pub fn configure_identity(dir: &Path) {
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
    configure_identity(&main.join("modules/sub"));
    main
}

pub fn perf_fixture(root: &Path, commits: usize, branches: usize) -> PathBuf {
    let repo = root.join("perf");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let mut stream = String::new();
    for index in 0..commits {
        let mark = index + 1;
        let message = format!("commit {mark}");
        let content = format!("content {mark}\n");
        stream.push_str("commit refs/heads/main\n");
        stream.push_str(&format!("mark :{mark}\n"));
        stream.push_str(&format!("author test <test@example.com> {mark} +0000\n"));
        stream.push_str(&format!("committer test <test@example.com> {mark} +0000\n"));
        stream.push_str(&format!("data {}\n{message}\n", message.len()));
        if index > 0 {
            stream.push_str(&format!("from :{index}\n"));
        }
        stream.push_str("M 100644 inline file.txt\n");
        stream.push_str(&format!("data {}\n{content}\n", content.len()));
    }
    stream.push_str("done\n");
    git_stdin(&repo, &["fast-import", "--quiet", "--done"], &stream);
    git(&repo, &["reset", "-q", "--hard", "main"]);

    let rev_list = git(&repo, &["rev-list", "main"]);
    let oids: Vec<&str> = rev_list.lines().collect();
    let stride = (oids.len() / branches.max(1)).max(1);
    let mut refs = String::new();
    for (slot, oid) in oids.iter().step_by(stride).take(branches).enumerate() {
        refs.push_str(&format!("create refs/heads/branch-{slot:03} {oid}\n"));
    }
    git_stdin(&repo, &["update-ref", "--stdin"], &refs);
    repo
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
