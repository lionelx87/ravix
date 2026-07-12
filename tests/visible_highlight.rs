use std::collections::HashSet;

use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::ui::render;

const NOW: i64 = 100_000;

fn commit(repo: &Repository, base: Option<Oid>, path: &str, body: &str) -> Oid {
    let signature = Signature::new("Dev", "dev@example.com", &Time::new(1000, 0)).unwrap();
    let blob = repo.blob(body.as_bytes()).unwrap();
    let base_tree = base.map(|oid| repo.find_commit(oid).unwrap().tree().unwrap());
    let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
    builder.insert(path, blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let parents: Vec<_> = base
        .into_iter()
        .map(|oid| repo.find_commit(oid).unwrap())
        .collect();
    let refs: Vec<_> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, "add rust", &tree, &refs)
        .unwrap()
}

fn rust_repo(dir: &TempDir) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir.path(), &opts).unwrap();
    let root = commit(&repo, None, "seed.txt", "seed\n");
    let mut body = String::new();
    for n in 0..120 {
        body.push_str(&format!("pub fn function_{n}(value: usize) -> usize {{ value + {n} }}\n"));
    }
    commit(&repo, Some(root), "lib.rs", &body);
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

fn draw(app: &mut App, w: u16, h: u16) -> Buffer {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, app, NOW)).unwrap();
    terminal.backend().buffer().clone()
}

fn distinct_diff_colors(buffer: &Buffer) -> usize {
    let area = buffer.area();
    let mut colors: HashSet<Color> = HashSet::new();
    for y in 0..area.height {
        for x in (area.width / 2)..area.width {
            colors.insert(buffer.cell((x, y)).unwrap().fg);
        }
    }
    colors.len()
}

#[test]
fn visible_diff_lines_are_syntax_highlighted() {
    let dir = TempDir::new().unwrap();
    rust_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    app.update(Action::OpenPanel); // fullscreen commit diff

    let top = draw(&mut app, 160, 30);
    assert!(
        distinct_diff_colors(&top) > 3,
        "the visible diff must carry multiple syntect colors, not a single plain color"
    );

    for _ in 0..60 {
        app.update(Action::ScrollDown);
    }
    let scrolled = draw(&mut app, 160, 30);
    assert!(
        distinct_diff_colors(&scrolled) > 3,
        "after scrolling, the newly visible lines are highlighted too (not left plain)"
    );
}
