use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::git::CommitInfo;
use crate::review::{BasePicker, BranchReview, ReviewFocus, ReviewRow};

use crate::app::App;

use super::{
    SELECTION_MARKER, Theme, centered, ease_out_cubic, relative_time, render_diff_pane,
    right_aligned, scroll_into_view, spinner_frame, stats_spans,
};

const ACCENT: Color = Color::Rgb(236, 128, 178);
const BAND_FG: Color = Color::Rgb(20, 24, 32);
const MERGE_RULE: &str = "┈┈";

const LIST_WIDTH: u16 = 56;
const MESSAGE_MAX_ROWS: u16 = 8;
const FILTER_MARKER: &str = "● ";

pub(super) fn render(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect, now: i64) {
    let notice = app
        .notice()
        .map(|text| notice_span(text, app.notice_is_error(), theme));
    let Some(review) = app.branch_review_mut() else {
        return;
    };
    let area = unfolded(area, review.unfold);
    frame.render_widget(Clear, area);
    if area.height < 3 {
        frame.render_widget(Paragraph::new(band_line(review, theme)), area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);
    let footer = right_aligned(
        keys_spans(review.is_filtered(), theme),
        notice.into_iter().collect(),
        rows[2].width as usize,
    );
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[2]);
    frame.render_widget(Paragraph::new(band_line(review, theme)), rows[0]);
    let columns =
        Layout::horizontal([Constraint::Length(LIST_WIDTH), Constraint::Min(0)]).split(rows[1]);
    let file_rows = (review.files().len() as u16 + 2)
        .max(3)
        .min(columns[0].height / 2);
    let left =
        Layout::vertical([Constraint::Min(0), Constraint::Length(file_rows)]).split(columns[0]);

    let title = commits_title(review);
    let commits_block = zone_block(title, review.focus == ReviewFocus::Commits, theme);
    let mut lines: Vec<Line> = review
        .visible_rows()
        .into_iter()
        .map(|row| match row {
            ReviewRow::Commit(commit) => {
                commit_line(commit, review.selected == Some(commit.id), theme, now)
            }
            ReviewRow::Merge(commit) => merge_line(commit, theme),
        })
        .collect();
    if review.rows.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  No own commits — already in {}", review.base),
            Style::default().fg(theme.meta),
        )));
    }
    frame.render_widget(Paragraph::new(lines).block(commits_block), left[0]);

    let files_block = zone_block(
        format!(" Files {} · ↵ filter ", review.files().len()),
        review.focus == ReviewFocus::Files,
        theme,
    );
    let mut file_lines: Vec<Line> = review
        .files()
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let focused = review.focus == ReviewFocus::Files && index == review.file;
            let filtering = review.file_filter.as_deref() == Some(file.path.as_str());
            let marker = match (focused, filtering) {
                (true, _) => SELECTION_MARKER,
                (false, true) => FILTER_MARKER,
                (false, false) => "  ",
            };
            let mut spans = vec![
                Span::styled(marker, Style::default().fg(ACCENT)),
                Span::styled(
                    format!("{} ", file.path),
                    Style::default().fg(if focused { theme.node } else { theme.summary }),
                ),
            ];
            spans.extend(stats_spans(
                crate::git::LineStats {
                    added: file.added,
                    removed: file.removed,
                },
                theme,
            ));
            Line::from(spans)
        })
        .collect();
    if review.summary_pending() {
        file_lines.push(Line::from(Span::styled(
            format!("  {} summarizing…", spinner_frame(review.spinner)),
            Style::default().fg(theme.meta),
        )));
    }
    let list_height = left[1].height.saturating_sub(2) as usize;
    let files_scroll = scroll_into_view(0, review.file, list_height, file_lines.len());
    frame.render_widget(
        Paragraph::new(file_lines)
            .block(files_block)
            .scroll((files_scroll, 0)),
        left[1],
    );

    let message_lines: Vec<Line> = review
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
        .min(columns[1].height / 2);
    let right =
        Layout::vertical([Constraint::Length(message_rows), Constraint::Min(0)]).split(columns[1]);
    let message_title = review
        .selected_commit()
        .map(|commit| format!(" {} · {} ", commit.short_id, commit.author_name))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(message_lines)
            .wrap(Wrap { trim: false })
            .block(zone_block(message_title, false, theme)),
        right[0],
    );

    if review.diff_pending() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {} loading the diff…", spinner_frame(review.spinner)),
                Style::default().fg(theme.meta),
            )))
            .block(zone_block(
                " Diff ".to_string(),
                review.focus == ReviewFocus::Diff,
                theme,
            )),
            right[1],
        );
        return;
    }
    render_diff_pane(
        frame,
        review.diff.as_ref(),
        review.split,
        None,
        &mut review.diff_scroll,
        &mut review.diff_hscroll,
        false,
        review.focus == ReviewFocus::Diff,
        theme,
        right[1],
    );
}

fn band_line(review: &BranchReview, theme: &Theme) -> Line<'static> {
    let provenance = if review.base_chosen { "chosen" } else { "auto" };
    Line::from(vec![
        Span::styled(
            " BRANCH REVIEW ",
            Style::default()
                .fg(BAND_FG)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} vs {} ({provenance}) ", review.branch, review.base),
            Style::default().fg(theme.node).add_modifier(Modifier::BOLD),
        ),
        summary_span(review, theme),
    ])
}

fn commit_line(commit: &CommitInfo, selected: bool, theme: &Theme, now: i64) -> Line<'static> {
    let marker = if selected { SELECTION_MARKER } else { "  " };
    let summary_style = if selected {
        Style::default().fg(theme.node).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.summary)
    };
    Line::from(vec![
        Span::styled(marker, Style::default().fg(ACCENT)),
        Span::styled(
            format!("{} ", commit.short_id),
            Style::default().fg(theme.short_id),
        ),
        Span::styled(
            format!("{:>4} ", relative_time(now, commit.time)),
            Style::default().fg(theme.meta),
        ),
        Span::styled(
            format!("{} ", commit.author_name),
            Style::default().fg(theme.label),
        ),
        Span::styled(commit.summary.clone(), summary_style),
    ])
}

fn merge_line(commit: &CommitInfo, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {MERGE_RULE} {} {MERGE_RULE}", commit.summary),
        Style::default().fg(theme.meta).add_modifier(Modifier::DIM),
    ))
}

fn summary_span(review: &BranchReview, theme: &Theme) -> Span<'static> {
    let Some(summary) = &review.summary else {
        return Span::styled(
            format!(" {} summarizing…", spinner_frame(review.spinner)),
            Style::default().fg(theme.meta),
        );
    };
    let count = |n: usize, word: &str| {
        let plural = if n == 1 { "" } else { "s" };
        format!("{n} {word}{plural}")
    };
    Span::styled(
        format!(
            " {} · {} · {} · +{} −{}",
            count(summary.commits, "commit"),
            count(summary.authors.len(), "author"),
            count(summary.files.len(), "file"),
            summary.added,
            summary.removed,
        ),
        Style::default().fg(theme.label),
    )
}

fn zone_block(title: String, focused: bool, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { ACCENT } else { theme.label }))
        .title(title)
}

pub(super) fn render_base_picker(
    frame: &mut Frame,
    picker: &BasePicker,
    theme: &Theme,
    area: Rect,
) {
    let width = area.width.clamp(20, 60);
    let height = (picker.rows().len() as u16 + 3)
        .clamp(5, 16)
        .min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(format!(" Review {} against… ", picker.branch));
    let mut lines = vec![Line::from(vec![
        Span::styled("base ", Style::default().fg(theme.label)),
        Span::styled(
            format!("{}▏", picker.query),
            Style::default().fg(theme.node),
        ),
    ])];
    lines.extend(picker.rows().into_iter().enumerate().map(|(index, name)| {
        let selected = index == picker.selected;
        Line::from(vec![
            Span::styled(
                if selected { SELECTION_MARKER } else { "  " },
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                name.to_string(),
                Style::default().fg(if selected { theme.node } else { theme.summary }),
            ),
        ])
    }));
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

const KEYS: &[(&str, &str)] = &[
    ("j/k", "move"),
    ("Tab", "zone"),
    ("v", "side-by-side"),
    ("b", "base"),
    ("/", "filter"),
    ("y", "copy hash"),
    ("?", "help"),
    ("Esc", "back"),
];

fn keys_spans(filtered: bool, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (key, label) in KEYS {
        let label = if *key == "Esc" && filtered {
            "clear filter"
        } else {
            label
        };
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().fg(theme.meta),
        ));
    }
    spans
}

fn notice_span(text: &str, error: bool, theme: &Theme) -> Span<'static> {
    let color = if error { theme.warn } else { ACCENT };
    Span::styled(format!("{text} "), Style::default().fg(color))
}

fn unfolded(area: Rect, unfold: f32) -> Rect {
    let height =
        ((f32::from(area.height) * ease_out_cubic(unfold)).round() as u16).clamp(1, area.height);
    Rect {
        y: area.y + (area.height - height) / 2,
        height,
        ..area
    }
}

fn commits_title(review: &BranchReview) -> String {
    let total = review.commits().count();
    let mut title = if review.is_filtered() {
        format!(" Own commits {} of {total}", review.visible_commit_count())
    } else {
        format!(" Own commits {total}")
    };
    if let Some(path) = &review.file_filter {
        title.push_str(&format!(" · {path}"));
    }
    if let Some(query) = &review.query {
        let caret = if review.query_editing { "▏" } else { "" };
        title.push_str(&format!(" · /{query}{caret}"));
    }
    title.push(' ');
    title
}
