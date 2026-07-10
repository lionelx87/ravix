use std::io;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use ogma::app::{Action, App};
use ogma::event::InputMap;
use ogma::ui::{regions, render};
use ogma::watcher::RepoWatcher;

const IDLE_POLL: Duration = Duration::from_millis(200);
const FRAME_POLL: Duration = Duration::from_millis(16);
const WATCH_DEBOUNCE: Duration = Duration::from_millis(120);

fn main() -> io::Result<()> {
    let app = match App::open(".") {
        Ok(app) => app,
        Err(error) => {
            eprintln!("og: {error}");
            std::process::exit(1);
        }
    };

    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    let result = run(&mut terminal, app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    let mut input = InputMap::default();
    let watcher = RepoWatcher::new(&app.git_dir(), WATCH_DEBOUNCE).ok();
    let mut last_tick = Instant::now();
    let mut graph_area = Rect::default();

    loop {
        terminal.draw(|frame| {
            graph_area = regions(frame.area()).0;
            render(frame, &mut app, now_seconds());
        })?;

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
                        app.update(action);
                    }
                }
                Event::Mouse(mouse) => {
                    if let Some(action) = input.on_mouse(mouse, graph_area) {
                        app.update(action);
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

        if watcher.as_ref().is_some_and(RepoWatcher::changed) {
            app.update(Action::Reload);
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
