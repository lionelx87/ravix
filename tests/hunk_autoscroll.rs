use std::path::Path;
use std::process::Command;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::ui::render;

const NOW: i64 = 100_000;

fn git_run(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn two_hunk_repo(dir: &TempDir) {
    let path = dir.path();
    let run = |args: &[&str]| git_run(path, args);
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "demo@ravix.dev"]);
    run(&["config", "user.name", "Demo"]);

    let original: String = (1..=80).map(|n| format!("aaaa {n}\n")).collect();
    std::fs::write(path.join("app.txt"), &original).unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "seed"]);

    let mut edited: Vec<String> = (1..=80).map(|n| format!("aaaa {n}\n")).collect();
    for line in edited.iter_mut().take(50).skip(4) {
        *line = "CHANGED top block\n".to_string();
    }
    edited[77] = "BOTTOM_EDIT\n".to_string();
    std::fs::write(path.join("app.txt"), edited.concat()).unwrap();
}

fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app, NOW)).unwrap();
    terminal.backend().buffer().clone()
}

fn dump(buffer: &Buffer) -> String {
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

fn enter_hunk_focus(app: &mut App) {
    app.update(Action::SelectPrev);
    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel);
    app.update(Action::ToggleFocus);
}

#[test]
fn hunk_navigation_scrolls_the_diff_in_side_by_side_too() {
    let dir = TempDir::new().unwrap();
    two_hunk_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_hunk_focus(&mut app);
    app.update(Action::ToggleDiffView); // side-by-side
    assert_eq!(app.working().unwrap().diff_scroll, 0);
    draw(&mut app, 120, 24);

    app.update(Action::SelectNext); // move to the far hunk
    draw(&mut app, 120, 24);
    assert!(
        app.working().unwrap().diff_scroll > 0,
        "in side-by-side, moving to an off-screen hunk still scrolls the diff"
    );
}

#[test]
fn jumping_to_an_offscreen_hunk_scrolls_it_into_view() {
    let dir = TempDir::new().unwrap();
    two_hunk_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    enter_hunk_focus(&mut app);
    let first = dump(&draw(&mut app, 120, 24));
    assert!(
        !first.contains("BOTTOM_EDIT"),
        "the far second hunk is below the fold before navigating:\n{first}"
    );
    assert_eq!(app.working().unwrap().diff_scroll, 0);

    app.update(Action::SelectNext);
    let second = dump(&draw(&mut app, 120, 24));
    assert!(
        app.working().unwrap().diff_scroll > 0,
        "moving to the off-screen hunk scrolls the diff down"
    );
    assert!(
        second.contains("BOTTOM_EDIT"),
        "the newly focused hunk is scrolled into view:\n{second}"
    );
}
