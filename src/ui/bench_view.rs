use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::app::App;
use crate::bench::{
    Arrival, BenchEnd, BenchFocus, BenchPicker, BenchRow, CherryPickBench, Pick, Prediction,
    RowKind,
};
use crate::git::CommitInfo;

use super::mode_chrome::{band_tag, keys_spans, notice_span, render_branch_picker, zone_block};
use super::{
    SELECTION_MARKER, Theme, ease_out_cubic, relative_time, right_aligned, scroll_into_view,
};

const ACCENT: Color = Color::Rgb(232, 72, 96);
const PICKED: &str = "● ";
const GHOST: &str = "◌ ";
const TIP: &str = "◉ ";
const COMMIT: &str = "○ ";
const EQUIVALENT: &str = "≡ ";

pub(super) fn render(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect, now: i64) {
    let notice = app
        .notice()
        .map(|text| notice_span(text, app.notice_is_error(), ACCENT, theme));
    let Some(bench) = app.cherry_pick_bench_mut() else {
        return;
    };
    let area = unfolded(area, bench.unfold);
    frame.render_widget(Clear, area);
    if area.height < 3 {
        frame.render_widget(Paragraph::new(band_line(bench, theme)), area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(Paragraph::new(band_line(bench, theme)), rows[0]);
    let footer = right_aligned(
        keys_spans(KEYS, ACCENT, theme),
        notice.into_iter().collect(),
        rows[2].width as usize,
    );
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[2]);

    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    render_source(frame, bench, theme, columns[0], now);
    render_destination(frame, bench, theme, columns[1], now);
}

fn render_source(
    frame: &mut Frame,
    bench: &mut CherryPickBench,
    theme: &Theme,
    area: Rect,
    now: i64,
) {
    let block = zone_block(
        format!(" Source · {} ", bench.source),
        bench.focus == BenchFocus::Source,
        ACCENT,
        theme,
    );
    let mut lines: Vec<Line> = bench
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            source_line(
                row,
                bench.focus == BenchFocus::Source && index == bench.selected,
                bench.is_picked(row.commit.id),
                theme,
                now,
            )
        })
        .collect();
    if bench.rows.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  No own commits on {}", bench.source),
            Style::default().fg(theme.meta),
        )));
    }
    let height = area.height.saturating_sub(2) as usize;
    bench.source_scroll =
        scroll_into_view(bench.source_scroll, bench.selected, height, lines.len());
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((bench.source_scroll, 0)),
        area,
    );
}

fn source_line(
    row: &BenchRow,
    selected: bool,
    picked: bool,
    theme: &Theme,
    now: i64,
) -> Line<'static> {
    let cursor = if selected { SELECTION_MARKER } else { "  " };
    let (mark, dim) = match row.kind {
        RowKind::Pickable if picked => (PICKED, false),
        RowKind::Pickable => ("  ", false),
        RowKind::Merge => ("┈ ", true),
        RowKind::AlreadyInDestination => (EQUIVALENT, true),
    };
    let mut summary = if selected {
        Style::default().fg(theme.node).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.summary)
    };
    if dim {
        summary = summary.fg(theme.meta).add_modifier(Modifier::DIM);
    }
    let mut spans = vec![
        Span::styled(cursor, Style::default().fg(ACCENT)),
        Span::styled(
            mark,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", row.commit.short_id),
            Style::default().fg(theme.short_id),
        ),
        Span::styled(row.commit.summary.clone(), summary),
        Span::styled(
            format!(
                "  {} {}",
                row.commit.author_name,
                relative_time(now, row.commit.time)
            ),
            Style::default().fg(theme.meta),
        ),
    ];
    if row.kind == RowKind::AlreadyInDestination {
        spans.push(Span::styled(
            "  already in destination",
            Style::default()
                .fg(theme.meta)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    Line::from(spans)
}

fn render_destination(
    frame: &mut Frame,
    bench: &mut CherryPickBench,
    theme: &Theme,
    area: Rect,
    now: i64,
) {
    let block = zone_block(
        format!(" Destination · {} ", bench.destination),
        bench.focus == BenchFocus::Destination,
        ACCENT,
        theme,
    );
    let mut lines: Vec<Line> = bench
        .ordered_picks()
        .into_iter()
        .rev()
        .map(|pick| {
            let prediction = (!bench.analysis_pending())
                .then(|| bench.prediction(pick.commit.id))
                .flatten();
            ghost_line(pick, prediction, theme)
        })
        .collect();
    let width = area.width.saturating_sub(2);
    lines.extend(
        bench
            .destination_log
            .iter()
            .enumerate()
            .map(|(index, commit)| {
                let arrival = bench
                    .landing
                    .as_ref()
                    .and_then(|landing| landing.arrival(commit.id));
                match arrival {
                    Some(Arrival::Flying(progress)) => flying_line(commit, progress, width, theme),
                    Some(Arrival::Landed) => landed_line(commit, theme),
                    None => destination_line(commit, index == 0, theme, now),
                }
            }),
    );
    let height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(height) as u16;
    bench.destination_scroll = bench.destination_scroll.min(max_scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((bench.destination_scroll, 0)),
        area,
    );
}

fn ghost_line(pick: &Pick, prediction: Option<&Prediction>, theme: &Theme) -> Line<'static> {
    let (verdict, color) = match prediction {
        Some(Prediction::Clean) => ("✓ clean".to_string(), theme.added),
        Some(Prediction::Conflicts(files)) => {
            let plural = if files.len() == 1 { "" } else { "s" };
            (
                format!("⚠ conflicts in {} file{plural}", files.len()),
                theme.warn,
            )
        }
        Some(Prediction::Failed) => ("✗ git could not replay it".to_string(), theme.warn),
        Some(Prediction::Pending) => ("· pending".to_string(), theme.meta),
        None => ("… predicting".to_string(), theme.meta),
    };
    Line::from(vec![
        Span::raw("  "),
        Span::styled(GHOST, Style::default().fg(ACCENT)),
        Span::styled(
            format!("{}' ", pick.commit.short_id),
            Style::default()
                .fg(theme.short_id)
                .add_modifier(Modifier::DIM),
        ),
        Span::styled(
            pick.commit.summary.clone(),
            Style::default().fg(ACCENT).add_modifier(Modifier::ITALIC),
        ),
        Span::styled(format!("  {verdict}"), Style::default().fg(color)),
    ])
}

fn flying_line(commit: &CommitInfo, progress: f32, width: u16, theme: &Theme) -> Line<'static> {
    let travel = f32::from(width / 2) * (1.0 - ease_out_cubic(progress));
    Line::from(vec![
        Span::raw(" ".repeat(travel.round() as usize + 2)),
        Span::styled(
            "➜ ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", commit.short_id),
            Style::default().fg(theme.short_id),
        ),
        Span::styled(
            commit.summary.clone(),
            Style::default().fg(ACCENT).add_modifier(Modifier::ITALIC),
        ),
    ])
}

fn landed_line(commit: &CommitInfo, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            PICKED,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", commit.short_id),
            Style::default()
                .fg(theme.short_id)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            commit.summary.clone(),
            Style::default().fg(theme.node).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ✓ copied", Style::default().fg(theme.added)),
    ])
}

fn destination_line(commit: &CommitInfo, tip: bool, theme: &Theme, now: i64) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            if tip { TIP } else { COMMIT },
            Style::default().fg(theme.label),
        ),
        Span::styled(
            format!("{} ", commit.short_id),
            Style::default().fg(theme.short_id),
        ),
        Span::styled(commit.summary.clone(), Style::default().fg(theme.summary)),
        Span::styled(
            format!("  {}", relative_time(now, commit.time)),
            Style::default().fg(theme.meta),
        ),
    ])
}

fn band_line(bench: &CherryPickBench, theme: &Theme) -> Line<'static> {
    let title = Style::default().fg(theme.node).add_modifier(Modifier::BOLD);
    Line::from(vec![
        band_tag("CHERRY-PICK BENCH", ACCENT),
        Span::styled(format!(" {} → {} ", bench.source, bench.destination), title),
        picks_span(bench, theme),
    ])
}

fn picks_span(bench: &CherryPickBench, theme: &Theme) -> Span<'static> {
    let count =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let text = if bench.picks.is_empty() {
        " · no picks".to_string()
    } else {
        format!(
            " · {} from {}",
            count(bench.picks.len(), "pick", "picks"),
            count(bench.branch_count(), "branch", "branches")
        )
    };
    Span::styled(text, Style::default().fg(theme.label))
}

pub(super) fn render_picker(frame: &mut Frame, picker: &BenchPicker, theme: &Theme, area: Rect) {
    let (title, prompt) = match picker.context.end {
        BenchEnd::Source => (" Cherry-pick from… ", "source"),
        BenchEnd::Destination => (" Cherry-pick onto… ", "destination"),
    };
    render_branch_picker(
        frame,
        picker,
        title.to_string(),
        prompt,
        ACCENT,
        theme,
        area,
    );
}

const KEYS: &[(&str, &str)] = &[
    ("j/k", "move"),
    ("␣", "pick"),
    ("a", "all"),
    ("Tab", "column"),
    ("b", "source"),
    ("B", "destination"),
    ("↵", "apply"),
    ("?", "help"),
    ("Esc", "back"),
];

fn unfolded(area: Rect, unfold: f32) -> Rect {
    let width =
        ((f32::from(area.width) * ease_out_cubic(unfold)).round() as u16).clamp(1, area.width);
    Rect {
        x: area.x + (area.width - width) / 2,
        width,
        ..area
    }
}
