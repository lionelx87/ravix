mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::TempDir;

use ravix::app::{Action, App};
use ravix::event::InputMap;
use ravix::ui::render;

const FRAME_BUDGET: Duration = Duration::from_millis(1);
const PANEL_BUDGET: Duration = Duration::from_micros(1_500);
const SCROLL_BUDGET: Duration = Duration::from_millis(2);
const FIXTURE_COMMITS: usize = 30_000;
const FIXTURE_BRANCHES: usize = 10_000;
const WARMUP_FRAMES: usize = 5;

fn bench_repo() -> (Option<TempDir>, PathBuf) {
    match std::env::var("RAVIX_BENCH_REPO") {
        Ok(path) => (None, PathBuf::from(path)),
        Err(_) => {
            let dir = TempDir::new().unwrap();
            let path = common::perf_fixture(dir.path(), FIXTURE_COMMITS, FIXTURE_BRANCHES);
            (Some(dir), path)
        }
    }
}

fn loaded_app(path: &Path) -> App {
    let mut app = App::open(path).unwrap();
    app.update(Action::SelectLast);
    app
}

fn bench_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(200, 50)).unwrap()
}

fn time_frames(
    app: &mut App,
    terminal: &mut Terminal<TestBackend>,
    frames: usize,
    mut step: impl FnMut(&mut App),
) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(frames);
    for _ in 0..frames {
        let start = Instant::now();
        step(app);
        terminal
            .draw(|frame| render(frame, app, common::NOW))
            .unwrap();
        samples.push(start.elapsed());
    }
    samples
}

fn report(name: &str, mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    let min = samples[0];
    let median = samples[samples.len() / 2];
    let p95 = samples[samples.len() * 95 / 100];
    let max = samples[samples.len() - 1];
    println!("{name}: min {min:?} median {median:?} p95 {p95:?} max {max:?}");
    median
}

#[test]
#[ignore]
fn frame_time_with_all_commits_loaded_stays_under_budget() {
    let (_dir, path) = bench_repo();
    let mut app = loaded_app(&path);
    let mut terminal = bench_terminal();
    time_frames(&mut app, &mut terminal, WARMUP_FRAMES, |_| {});
    let samples = time_frames(&mut app, &mut terminal, 100, |_| {});
    let median = report("full-graph frame", samples);
    assert!(
        median < FRAME_BUDGET,
        "median frame time {median:?} exceeds {FRAME_BUDGET:?}"
    );
}

#[test]
#[ignore]
fn commit_navigation_keypress_latency_stays_under_budget() {
    let (_dir, path) = bench_repo();
    let mut app = loaded_app(&path);
    let mut input = InputMap::default();
    let mut terminal = bench_terminal();
    time_frames(&mut app, &mut terminal, WARMUP_FRAMES, |_| {});
    let mut up = false;
    let samples = time_frames(&mut app, &mut terminal, 200, |app| {
        up = !up;
        let code = if up {
            KeyCode::Char('k')
        } else {
            KeyCode::Char('j')
        };
        common::press(app, &mut input, code);
    });
    let median = report("commit j/k keypress", samples);
    assert!(
        median < FRAME_BUDGET,
        "median keypress latency {median:?} exceeds {FRAME_BUDGET:?}"
    );
}

#[test]
#[ignore]
fn scroll_from_top_through_page_loads_stays_under_budget() {
    let (_dir, path) = bench_repo();
    let mut app = App::open(&path).unwrap();
    let mut input = InputMap::default();
    let mut terminal = bench_terminal();
    time_frames(&mut app, &mut terminal, WARMUP_FRAMES, |_| {});
    let samples = time_frames(&mut app, &mut terminal, 1000, |app| {
        common::press(app, &mut input, KeyCode::Char('j'));
    });
    let median = report("scroll from top", samples);
    assert!(
        median < SCROLL_BUDGET,
        "median scroll latency {median:?} exceeds {SCROLL_BUDGET:?}"
    );
}

#[test]
#[ignore]
fn peek_navigation_keypress_latency_stays_under_budget() {
    let (_dir, path) = bench_repo();
    let mut app = App::open(&path).unwrap();
    let mut input = InputMap::default();
    let mut terminal = bench_terminal();
    common::press(&mut app, &mut input, KeyCode::Enter);
    app.update(Action::Tick(Duration::from_secs(2)));
    time_frames(&mut app, &mut terminal, WARMUP_FRAMES, |_| {});
    let samples = time_frames(&mut app, &mut terminal, 200, |app| {
        common::press(app, &mut input, KeyCode::Char('j'));
        app.update(Action::Tick(Duration::from_millis(16)));
    });
    let median = report("peek panel j/k keypress", samples);
    assert!(
        median < SCROLL_BUDGET,
        "median peek navigation latency {median:?} exceeds {SCROLL_BUDGET:?}"
    );
}

#[test]
#[ignore]
fn branch_panel_frame_time_stays_under_budget() {
    let (_dir, path) = bench_repo();
    let mut app = App::open(&path).unwrap();
    app.update(Action::ToggleBranches);
    app.update(Action::Tick(Duration::from_secs(2)));
    let mut input = InputMap::default();
    let mut terminal = bench_terminal();
    time_frames(&mut app, &mut terminal, WARMUP_FRAMES, |_| {});
    let mut up = false;
    let samples = time_frames(&mut app, &mut terminal, 200, |app| {
        up = !up;
        let code = if up {
            KeyCode::Char('k')
        } else {
            KeyCode::Char('j')
        };
        common::press(app, &mut input, code);
    });
    let median = report("branch panel j/k keypress", samples);
    assert!(
        median < PANEL_BUDGET,
        "median branch panel latency {median:?} exceeds {PANEL_BUDGET:?}"
    );
}
