use std::io;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use ravix::app::{Action, App};
use ravix::enrich;
use ravix::event::InputMap;
use ravix::ui::{regions, render};
use ravix::watcher::RepoWatcher;

const IDLE_POLL: Duration = Duration::from_millis(200);
const FRAME_POLL: Duration = Duration::from_millis(16);
const WATCH_DEBOUNCE: Duration = Duration::from_millis(120);
const STATUS_FALLBACK: Duration = Duration::from_millis(500);

fn main() -> io::Result<()> {
    if let Some(prompt) = ravix::askpass::helper_prompt() {
        std::process::exit(i32::from(ravix::askpass::run_helper(&prompt).is_err()));
    }

    let app = match App::open(".") {
        Ok(app) => app,
        Err(error) => {
            eprintln!("og: {error}");
            std::process::exit(1);
        }
    };

    std::thread::spawn(enrich::warm_up);

    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    let result = run(&mut terminal, app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

struct PerfLog {
    out: Option<std::io::BufWriter<std::fs::File>>,
    started: Instant,
}

impl PerfLog {
    fn from_env() -> Self {
        let out = std::env::var("RAVIX_PERF_LOG")
            .ok()
            .and_then(|path| std::fs::File::create(path).ok())
            .map(std::io::BufWriter::new);
        Self {
            out,
            started: Instant::now(),
        }
    }

    fn record(&mut self, label: &str, duration: Duration) {
        use std::io::Write;
        if let Some(out) = &mut self.out {
            let at = self.started.elapsed().as_millis();
            let _ = writeln!(out, "{at},{label},{}", duration.as_micros());
            let _ = out.flush();
        }
    }
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    let mut input = InputMap::default();
    let watcher = RepoWatcher::new(&app.git_dir(), app.workdir().as_deref(), WATCH_DEBOUNCE);
    let mut last_tick = Instant::now();
    let mut last_status = Instant::now();
    let mut watch_warned = false;
    let mut graph_area = Rect::default();
    let mut perf = PerfLog::from_env();

    loop {
        let draw_started = Instant::now();
        terminal.draw(|frame| {
            graph_area = regions(frame.area()).0;
            render(frame, &mut app, now_seconds());
        })?;
        perf.record("draw", draw_started.elapsed());

        if app.should_quit() {
            return Ok(());
        }

        let timeout = if app.is_animating() {
            FRAME_POLL
        } else {
            IDLE_POLL
        };

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(action) = input.on_key(key, app.input_context()) {
                        let update_started = Instant::now();
                        app.update(action);
                        perf.record("key", update_started.elapsed());
                    }
                }
                Event::Mouse(mouse) => {
                    let context = app.input_context();
                    if let Some(action) = input.on_mouse(mouse, graph_area, context) {
                        let update_started = Instant::now();
                        app.update(action);
                        perf.record("mouse", update_started.elapsed());
                    }
                }
                _ => {}
            }
        }

        if let Some(path) = app.pending_edit_path() {
            edit_in_editor(terminal, &path)?;
            app.finish_edit();
            last_tick = Instant::now();
            continue;
        }

        let elapsed = last_tick.elapsed();
        last_tick = Instant::now();
        if app.is_animating() {
            app.update(Action::Tick(elapsed));
        }

        app.poll_remote();
        app.poll_askpass();

        if !watch_warned && watcher.degraded() {
            app.report_watch_degraded();
            watch_warned = true;
        }

        if watcher.changed() {
            let reload_started = Instant::now();
            app.update(Action::Reload);
            perf.record("reload", reload_started.elapsed());
            last_status = Instant::now();
        } else if last_status.elapsed() >= STATUS_FALLBACK {
            let poll_started = Instant::now();
            app.poll_status();
            perf.record("status", poll_started.elapsed());
            last_status = Instant::now();
        }
    }
}

fn edit_in_editor(terminal: &mut DefaultTerminal, path: &std::path::Path) -> io::Result<()> {
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(editor).arg(path).status();
    *terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    terminal.clear()?;
    status.map(|_| ())
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
