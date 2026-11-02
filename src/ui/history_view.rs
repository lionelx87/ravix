use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use git2::Oid;

use crate::app::App;
use crate::history::{FileFinder, FileHistory, HistoryFocus, HistoryRow, RowKey, change_bar};

use super::graph_view::clip_pad;
use super::mode_chrome::{band_tag, keys_spans, notice_span, render_branch_picker, zone_block};
use super::{
    SELECTION_MARKER, Theme, ease_out_cubic, flashed, relative_time, render_diff_pane,
    right_aligned, scroll_into_view, spinner_frame,
};

const ACCENT: Color = Color::Rgb(86, 204, 186);
const LIST_WIDTH: u16 = 72;
const MESSAGE_MAX_ROWS: u16 = 8;
const AUTHOR_WIDTH: usize = 12;
const BAR_WIDTH: usize = 10;
const ANCHOR_MARKER: &str = "◆ ";
const RANGE_MARKER: &str = "┃ ";
const MERGE_MARK: &str = "⇢ ";
const RENAME_MARK: &str = "↳ ";

pub(super) fn render(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect, now: i64) {
    let notice = app
        .notice()
        .map(|text| notice_span(text, app.notice_is_error(), ACCENT, theme));
    let flashed_hash = app.flashed_hash().map(str::to_string);
    let Some(history) = app.file_history_mut() else {
        return;
    };
    let area = sweeping_in(area, history.unfold);
    frame.render_widget(Clear, area);
    if area.height < 3 || area.width < 20 {
        frame.render_widget(Paragraph::new(band_line(history, theme)), area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(Paragraph::new(band_line(history, theme)), rows[0]);
    let footer = right_aligned(
        keys_spans(&history_keys(history), ACCENT, theme),
        notice.into_iter().collect(),
        rows[2].width as usize,
    );
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[2]);

    let list_width = LIST_WIDTH.min(rows[1].width / 2);
    let columns =
        Layout::horizontal([Constraint::Length(list_width), Constraint::Min(0)]).split(rows[1]);
    render_commits(
        frame,
        history,
        flashed_hash.as_deref(),
        theme,
        columns[0],
        now,
    );
    render_detail(frame, history, theme, columns[1]);
}

fn render_commits(
    frame: &mut Frame,
    history: &mut FileHistory,
    flashed_hash: Option<&str>,
    theme: &Theme,
    area: Rect,
    now: i64,
) {
    let inner_width = area.width.saturating_sub(2) as usize;
    let largest = history.largest_change();
    let visible = history.visible_rows();
    let mut lines: Vec<Line> = Vec::new();
    let mut selected_line = 0;
    for row in &visible {
        if history.selected == Some(row.key()) {
            selected_line = lines.len();
        }
        lines.extend({
            let key = row.key();
            let marker = if history.selected == Some(key) {
                SELECTION_MARKER
            } else if history.anchor == Some(key) {
                ANCHOR_MARKER
            } else if history.in_anchor_range(key) {
                RANGE_MARKER
            } else {
                "  "
            };
            let flashing = flashed_hash.is_some_and(|hash| hash == row.short_id());
            row_lines(
                row,
                RowLook {
                    marker,
                    selected: history.selected == Some(key),
                    flashing,
                    largest,
                    width: inner_width,
                },
                theme,
                now,
            )
        });
    }
    drop(visible);
    if history.loading() {
        lines.push(Line::from(Span::styled(
            format!("  {} searching…", spinner_frame(history.spinner)),
            Style::default().fg(theme.meta),
        )));
    } else if history.rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No commits changed this file",
            Style::default().fg(theme.meta),
        )));
    }
    let list_height = area.height.saturating_sub(2) as usize;
    history.commits_scroll = scroll_into_view(
        history.commits_scroll,
        selected_line,
        list_height,
        lines.len(),
    );
    let block = zone_block(
        commits_title(history),
        history.focus == HistoryFocus::Commits,
        ACCENT,
        theme,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((history.commits_scroll, 0)),
        area,
    );
}

struct RowLook {
    marker: &'static str,
    selected: bool,
    flashing: bool,
    largest: usize,
    width: usize,
}

fn row_lines(row: &HistoryRow<Oid>, look: RowLook, theme: &Theme, now: i64) -> Vec<Line<'static>> {
    let summary_style = if look.selected {
        Style::default().fg(theme.node).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.summary)
    };
    let mut spans = vec![Span::styled(look.marker, Style::default().fg(ACCENT))];
    let fixed = 2 + 8 + 5 + AUTHOR_WIDTH + 1 + BAR_WIDTH + 1;
    let subject_width = look.width.saturating_sub(fixed);
    match row.key() {
        RowKey::Uncommitted => {
            spans.push(Span::styled(
                format!("{:<8}", "◌ wip"),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" ".repeat(5 + AUTHOR_WIDTH + 1)));
            spans.push(Span::styled(
                clip_pad("Uncommitted changes", subject_width),
                Style::default().fg(ACCENT).add_modifier(Modifier::ITALIC),
            ));
        }
        RowKey::Commit(_) => {
            spans.push(Span::styled(
                format!("{} ", row.short_id()),
                flashed(Style::default().fg(theme.short_id), look.flashing),
            ));
            spans.push(Span::styled(
                format!("{:>4} ", relative_time(now, row.time)),
                Style::default().fg(theme.meta),
            ));
            spans.push(Span::styled(
                format!("{} ", clip_pad(&row.author, AUTHOR_WIDTH)),
                Style::default().fg(theme.label),
            ));
            let mark = if row.is_merge() { MERGE_MARK } else { "" };
            spans.push(Span::styled(mark, Style::default().fg(theme.short_id)));
            let room = subject_width.saturating_sub(mark.chars().count());
            spans.push(Span::styled(clip_pad(&row.subject, room), summary_style));
        }
    }
    spans.push(Span::raw(" "));
    spans.extend(bar_spans(row, look.largest, theme));
    let mut lines = vec![Line::from(spans)];
    if let Some(old) = &row.renamed_from {
        let gutter = if look.marker == RANGE_MARKER {
            RANGE_MARKER
        } else {
            "  "
        };
        lines.push(Line::from(vec![
            Span::styled(gutter, Style::default().fg(ACCENT)),
            Span::raw(" ".repeat(8 + 5 + AUTHOR_WIDTH + 1)),
            Span::styled(
                format!("{RENAME_MARK}renamed from {old}"),
                Style::default().fg(ACCENT).add_modifier(Modifier::DIM),
            ),
        ]));
    }
    lines
}

fn bar_spans(row: &HistoryRow<Oid>, largest: usize, theme: &Theme) -> Vec<Span<'static>> {
    if row.binary {
        return vec![Span::styled(
            format!("{:<BAR_WIDTH$}", "bin"),
            Style::default().fg(theme.meta),
        )];
    }
    let change = row.added + row.removed;
    let length = change_bar(change, largest, BAR_WIDTH);
    let added = if change == 0 {
        0
    } else {
        (length * row.added).div_ceil(change).min(length)
    };
    vec![
        Span::styled("█".repeat(added), Style::default().fg(theme.added)),
        Span::styled(
            "█".repeat(length - added),
            Style::default().fg(theme.removed),
        ),
        Span::raw(" ".repeat(BAR_WIDTH - length)),
    ]
}

fn render_detail(frame: &mut Frame, history: &mut FileHistory, theme: &Theme, area: Rect) {
    let message_lines: Vec<Line> = history
        .message
        .lines()
        .enumerate()
        .map(|(index, text)| {
            let style = if index == 0 {
                Style::default().fg(theme.node).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.summary)
            };
            Line::from(Span::styled(text.to_string(), style))
        })
        .collect();
    let message_rows = (message_lines.len() as u16 + 2)
        .min(MESSAGE_MAX_ROWS)
        .min(area.height / 2);
    let right =
        Layout::vertical([Constraint::Length(message_rows), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(message_lines)
            .wrap(Wrap { trim: false })
            .block(zone_block(detail_title(history), false, ACCENT, theme)),
        right[0],
    );

    if history.diff_pending() || (history.diff.is_none() && history.selected.is_some()) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {} loading the diff…", spinner_frame(history.spinner)),
                Style::default().fg(theme.meta),
            )))
            .block(zone_block(
                " Diff ".to_string(),
                history.focus == HistoryFocus::Diff,
                ACCENT,
                theme,
            )),
            right[1],
        );
        return;
    }
    let snap = std::mem::take(&mut history.hunk_snap);
    render_diff_pane(
        frame,
        history.diff.as_ref(),
        history.split,
        history.hunk,
        &mut history.diff_scroll,
        &mut history.diff_hscroll,
        snap,
        history.focus == HistoryFocus::Diff,
        theme,
        right[1],
    );
}

fn detail_title(history: &FileHistory) -> String {
    let Some(selected) = history.selected_row() else {
        return String::new();
    };
    match history
        .anchor_row()
        .filter(|anchor| anchor.key() != selected.key())
    {
        Some(anchor) => {
            let anchor_index = history
                .rows
                .iter()
                .position(|row| row.key() == anchor.key());
            let selected_index = history
                .rows
                .iter()
                .position(|row| row.key() == selected.key());
            let (older, newer) = if anchor_index > selected_index {
                (anchor, selected)
            } else {
                (selected, anchor)
            };
            format!(
                " {} → {} · accumulated change ",
                older.version_name(),
                newer.version_name()
            )
        }
        None if selected.id.is_none() => " HEAD → working tree ".to_string(),
        None => format!(" {} · {} ", selected.short_id(), selected.author),
    }
}

fn band_line(history: &FileHistory, theme: &Theme) -> Line<'static> {
    let title = Style::default().fg(theme.node).add_modifier(Modifier::BOLD);
    let commits = history.commit_count();
    let plural = if commits == 1 { "" } else { "s" };
    let mut spans = vec![
        band_tag("FILE HISTORY", ACCENT),
        Span::styled(format!(" {} ", history.path), title),
        Span::styled(
            format!("· {commits} commit{plural} "),
            Style::default().fg(theme.label),
        ),
    ];
    if let Some(from) = &history.from {
        spans.push(Span::styled(
            format!("· from {from} "),
            Style::default().fg(ACCENT),
        ));
    }
    if history.loading() {
        spans.push(Span::styled(
            format!("{} searching…", spinner_frame(history.spinner)),
            Style::default().fg(theme.meta),
        ));
    }
    Line::from(spans)
}

fn commits_title(history: &FileHistory) -> String {
    let mut title = String::from(" Commits");
    if let Some(query) = &history.query {
        let caret = if history.query_editing { "▏" } else { "" };
        title.push_str(&format!(" · /{query}{caret}"));
    }
    if let Some(anchor) = history.anchor_row() {
        let name = match anchor.id {
            Some(_) => format!("{} {}", anchor.short_id(), anchor.subject),
            None => anchor.version_name(),
        };
        title.push_str(&format!(" · {ANCHOR_MARKER}{name}"));
    }
    title.push(' ');
    title
}

fn history_keys(history: &FileHistory) -> Vec<(&'static str, &'static str)> {
    let escape = if history.is_filtered() {
        "clear filter"
    } else if history.anchor.is_some() {
        "drop anchor"
    } else {
        "back"
    };
    let full = if history.full_file {
        "diff"
    } else {
        "full file"
    };
    vec![
        ("j/k", "move"),
        ("gg/G", "first/last"),
        ("Tab", "zone"),
        ("Space", "anchor"),
        ("f", full),
        ("n/N", "change"),
        ("v", "side-by-side"),
        ("/", "filter"),
        ("↵", "graph"),
        ("?", "help"),
        ("Esc", escape),
    ]
}

pub(super) fn render_finder(frame: &mut Frame, finder: &FileFinder, theme: &Theme, area: Rect) {
    render_branch_picker(
        frame,
        finder,
        " File history of… ".to_string(),
        "file",
        ACCENT,
        theme,
        area,
    );
}

fn sweeping_in(area: Rect, unfold: f32) -> Rect {
    let width =
        ((f32::from(area.width) * ease_out_cubic(unfold)).round() as u16).clamp(1, area.width);
    Rect { width, ..area }
}
