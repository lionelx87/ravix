use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, CommitEditor, Confirm, Panel};
use crate::git::BadgeKind;
use crate::working::{Focus, WorkingView};

const SELECTION_MARKER: &str = "❯ ";

pub struct Theme {
    lanes: [Color; 8],
    selection_bg: Color,
    marker: Color,
    node: Color,
    short_id: Color,
    summary: Color,
    meta: Color,
    head_badge: Color,
    branch_badge: Color,
    upstream_badge: Color,
    status_bg: Color,
    status_fg: Color,
    panel_border: Color,
    label: Color,
    added: Color,
    removed: Color,
    untracked: Color,
    wip: Color,
    warn: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            lanes: [
                Color::Rgb(97, 175, 239),
                Color::Rgb(224, 108, 117),
                Color::Rgb(152, 195, 121),
                Color::Rgb(198, 120, 221),
                Color::Rgb(229, 192, 123),
                Color::Rgb(86, 182, 194),
                Color::Rgb(224, 145, 66),
                Color::Rgb(171, 178, 191),
            ],
            selection_bg: Color::Rgb(45, 55, 72),
            marker: Color::Rgb(97, 175, 239),
            node: Color::Rgb(220, 223, 228),
            short_id: Color::Rgb(229, 192, 123),
            summary: Color::Rgb(220, 223, 228),
            meta: Color::Rgb(120, 128, 140),
            head_badge: Color::Rgb(152, 195, 121),
            branch_badge: Color::Rgb(97, 175, 239),
            upstream_badge: Color::Rgb(120, 128, 140),
            status_bg: Color::Rgb(33, 40, 54),
            status_fg: Color::Rgb(171, 178, 191),
            panel_border: Color::Rgb(97, 175, 239),
            label: Color::Rgb(120, 128, 140),
            added: Color::Rgb(152, 195, 121),
            removed: Color::Rgb(224, 108, 117),
            untracked: Color::Rgb(229, 192, 123),
            wip: Color::Rgb(229, 192, 123),
            warn: Color::Rgb(224, 108, 117),
        }
    }
}

impl Theme {
    fn lane(&self, lane: usize) -> Color {
        self.lanes[lane % self.lanes.len()]
    }

    fn badge_style(&self, kind: BadgeKind) -> Style {
        let color = match kind {
            BadgeKind::Head => self.head_badge,
            BadgeKind::LocalBranch => self.branch_badge,
            BadgeKind::Upstream => self.upstream_badge,
        };
        Style::default()
            .fg(Color::Rgb(20, 24, 32))
            .bg(color)
            .add_modifier(Modifier::BOLD)
    }
}

pub fn regions(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
    (chunks[0], chunks[1])
}

pub fn render(frame: &mut Frame, app: &mut App, now: i64) {
    let theme = Theme::default();
    let area = frame.area();
    let (graph_area, status_area) = regions(area);

    let wip_rows = u16::from(app.has_wip());
    let commit_area = Rect {
        x: graph_area.x,
        y: graph_area.y + wip_rows,
        width: graph_area.width,
        height: graph_area.height.saturating_sub(wip_rows),
    };
    app.set_viewport(commit_area.height as usize);

    render_graph(frame, app, &theme, commit_area, now);
    if app.has_wip() {
        let wip_area = Rect {
            x: graph_area.x,
            y: graph_area.y,
            width: graph_area.width,
            height: 1,
        };
        render_wip_row(frame, app, &theme, wip_area);
    }
    render_status(frame, app, &theme, status_area);

    if let Some(view) = app.working() {
        if view.fullscreen {
            render_working_fullscreen(frame, app, view, &theme, graph_area);
        } else {
            render_working_panel(frame, app, view, &theme, graph_area);
        }
    }
    if let Some(panel) = app.panel() {
        render_panel(frame, app, panel, &theme, graph_area);
    }
    if let Some(editor) = app.commit_editor() {
        render_commit_editor(frame, editor, &theme, area);
    }
    if let Some(confirm) = app.confirm() {
        render_confirm(frame, confirm, &theme, area);
    }
    if app.help_visible() {
        render_help(frame, &theme, area);
    }
}

fn render_graph(frame: &mut Frame, app: &App, theme: &Theme, area: Rect, now: i64) {
    if area.height == 0 {
        return;
    }
    let commits = app.commits();
    let rows = app.rows();
    let offset = app.offset();
    let selected = app.selected();

    if commits.is_empty() {
        let empty = Paragraph::new("No commits to show").style(Style::default().fg(theme.meta));
        frame.render_widget(empty, area);
        return;
    }

    let badges = &app.meta().badges;
    let buffer = frame.buffer_mut();

    for row in 0..area.height as usize {
        let index = offset + row;
        if index >= commits.len() {
            break;
        }
        let commit = &commits[index];
        let graph_row = &rows[index];
        let is_selected = index == selected;
        let y = area.y + row as u16;

        let mut spans = Vec::new();
        spans.push(Span::styled(
            if is_selected { SELECTION_MARKER } else { "  " },
            Style::default().fg(theme.marker),
        ));

        for (column, glyph) in graph_row.glyphs.chars().enumerate() {
            let color = if glyph == '●' {
                theme.node
            } else {
                theme.lane(column / 2)
            };
            spans.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
        }
        spans.push(Span::raw("  "));

        if let Some(commit_badges) = badges.get(&commit.id) {
            for badge in commit_badges {
                spans.push(Span::styled(
                    format!(" {} ", badge.label),
                    theme.badge_style(badge.kind),
                ));
                spans.push(Span::raw(" "));
            }
        }

        spans.push(Span::styled(
            commit.short_id.clone(),
            Style::default().fg(theme.short_id),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            commit.summary.clone(),
            Style::default().fg(theme.summary),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(
                "{} · {}",
                commit.author_name,
                relative_time(now, commit.time)
            ),
            Style::default().fg(theme.meta),
        ));

        buffer.set_line(area.x, y, &Line::from(spans), area.width);

        if is_selected {
            let highlight = Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            };
            buffer.set_style(
                highlight,
                Style::default()
                    .bg(theme.selection_bg)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

fn render_status(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let meta = app.meta();
    let branch = meta.head_branch.as_deref().unwrap_or("detached HEAD");
    let position = format!("{}/{}", app.selected() + 1, app.commits().len().max(1));

    let mut spans = vec![
        Span::raw(" "),
        Span::styled(
            meta.name.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  ⎇ "),
        Span::styled(branch.to_string(), Style::default().fg(theme.branch_badge)),
        Span::raw("  "),
        Span::raw(position),
    ];
    if let Some(notice) = app.notice() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("⚠ {notice}"),
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::raw("  ? help  q quit"));
    }

    let status = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(theme.status_bg).fg(theme.status_fg));
    frame.render_widget(status, area);
}

fn render_panel(frame: &mut Frame, app: &App, panel: &Panel, theme: &Theme, area: Rect) {
    let eased = ease_out_cubic(panel.progress);
    let full_width = ((area.width as f32) * 0.45)
        .max(30.0)
        .min(area.width as f32) as u16;
    let visible = ((full_width as f32) * eased).round() as u16;
    if visible < 4 {
        return;
    }

    let rect = Rect {
        x: area.right() - visible,
        y: area.y,
        width: visible,
        height: area.height,
    };
    frame.render_widget(Clear, rect);

    let Some(commit) = app.commits().get(panel.commit_index) else {
        return;
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Commit ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = vec![
        Line::from(Span::styled(
            commit.short_id.clone(),
            Style::default()
                .fg(theme.short_id)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Author  ", Style::default().fg(theme.label)),
            Span::raw(format!("{} <{}>", commit.author_name, commit.author_email)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            commit.summary.clone(),
            Style::default().fg(theme.summary),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Files",
            Style::default()
                .fg(theme.label)
                .add_modifier(Modifier::BOLD),
        )),
    ];
    for file in &panel.changed_files {
        lines.push(Line::from(format!(" {} {}", file.status, file.path)));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn render_help(frame: &mut Frame, theme: &Theme, area: Rect) {
    let bindings = [
        ("j / ↓", "select next commit"),
        ("k / ↑", "select previous commit"),
        ("gg / G", "jump to first / last"),
        ("PgUp / PgDn", "page up / down"),
        ("Enter", "open commit detail panel"),
        ("Esc", "close panel or help"),
        ("?", "toggle this help"),
        ("q", "quit"),
    ];

    let width = 44u16.min(area.width);
    let height = (bindings.len() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);

    let lines: Vec<Line> = bindings
        .iter()
        .map(|(keys, description)| {
            Line::from(vec![
                Span::styled(
                    format!(" {keys:<12}"),
                    Style::default()
                        .fg(theme.branch_badge)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(description.to_string()),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Help ");
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

fn render_wip_row(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let status = app.status();
    let mut spans = vec![
        Span::styled(
            if app.on_wip() { "❯ " } else { "  " },
            Style::default().fg(theme.marker),
        ),
        Span::styled(
            "◇",
            Style::default().fg(theme.wip).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  Uncommitted changes",
            Style::default().fg(theme.node).add_modifier(Modifier::BOLD),
        ),
    ];
    if !status.staged.is_empty() {
        spans.push(Span::styled(
            format!("   {} staged", status.staged.len()),
            Style::default().fg(theme.added),
        ));
    }
    if !status.unstaged.is_empty() {
        spans.push(Span::styled(
            format!("   {} unstaged", status.unstaged.len()),
            Style::default().fg(theme.removed),
        ));
    }
    if !status.untracked.is_empty() {
        spans.push(Span::styled(
            format!("   {} untracked", status.untracked.len()),
            Style::default().fg(theme.untracked),
        ));
    }

    let buffer = frame.buffer_mut();
    buffer.set_line(area.x, area.y, &Line::from(spans), area.width);
    if app.on_wip() {
        buffer.set_style(
            area,
            Style::default()
                .bg(theme.selection_bg)
                .add_modifier(Modifier::BOLD),
        );
    }
}

fn working_file_lines(app: &App, view: &WorkingView, theme: &Theme) -> Vec<Line<'static>> {
    let status = app.status();
    let sections = [
        ("Unstaged", &status.unstaged, theme.removed),
        ("Staged", &status.staged, theme.added),
        ("Untracked", &status.untracked, theme.untracked),
    ];

    let mut lines = Vec::new();
    let mut index = 0usize;
    for (title, files, color) in sections {
        if files.is_empty() {
            continue;
        }
        lines.push(section_title(title, files.len(), theme));
        for file in files {
            let selected = index == view.selected;
            lines.push(Line::from(vec![
                Span::styled(
                    if selected { "❯ " } else { "  " },
                    Style::default().fg(theme.marker),
                ),
                Span::styled(
                    format!("{} ", file.status),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    file.path.clone(),
                    Style::default().fg(if selected { theme.node } else { theme.summary }),
                ),
            ]));
            index += 1;
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "working tree clean",
            Style::default().fg(theme.meta),
        )));
    }
    lines
}

fn diff_lines(view: &WorkingView, theme: &Theme) -> Vec<Line<'static>> {
    let Some(diff) = view.diff.as_ref() else {
        return vec![Line::from(Span::styled(
            "No textual diff — press Tab to stage hunks",
            Style::default().fg(theme.meta),
        ))];
    };

    let mut lines = Vec::new();
    for (index, hunk) in diff.hunks.iter().enumerate() {
        let focused = view.focus == Focus::Hunks && index == view.hunk;
        lines.push(Line::from(vec![
            Span::styled(
                if focused { "❯ " } else { "  " },
                Style::default().fg(theme.marker),
            ),
            Span::styled(
                hunk.header.clone(),
                Style::default()
                    .fg(theme.branch_badge)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        for line in &hunk.lines {
            let color = match line.chars().next() {
                Some('+') => theme.added,
                Some('-') => theme.removed,
                _ => theme.meta,
            };
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(color),
            )));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn section_title(title: &str, count: usize, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{title} "),
            Style::default()
                .fg(theme.branch_badge)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("({count})"), Style::default().fg(theme.meta)),
    ])
}

fn render_working_panel(
    frame: &mut Frame,
    app: &App,
    view: &WorkingView,
    theme: &Theme,
    area: Rect,
) {
    let eased = ease_out_cubic(view.slide);
    let full_width = ((area.width as f32) * 0.5).max(46.0).min(area.width as f32) as u16;
    let visible = ((full_width as f32) * eased).round() as u16;
    if visible < 6 {
        return;
    }

    let rect = Rect {
        x: area.right() - visible,
        y: area.y,
        width: visible,
        height: area.height,
    };
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Uncommitted changes   [Enter] fullscreen ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = working_file_lines(app, view, theme);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "── diff ───────────────",
        Style::default().fg(theme.meta),
    )));
    lines.extend(diff_lines(view, theme));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[space] stage  [Tab] hunks  [d] discard  [c] commit",
        Style::default().fg(theme.meta),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_working_fullscreen(
    frame: &mut Frame,
    app: &App,
    view: &WorkingView,
    theme: &Theme,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let columns = Layout::horizontal([Constraint::Length(46), Constraint::Min(0)]).split(area);

    let files_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Working directory   [space] stage · [Tab] hunks · [c] commit ");
    frame.render_widget(
        Paragraph::new(working_file_lines(app, view, theme)).block(files_block),
        columns[0],
    );

    let title = view
        .diff
        .as_ref()
        .map(|diff| format!(" {} ", diff.new_path))
        .unwrap_or_else(|| " Diff ".to_string());
    let diff_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.label))
        .title(title);
    frame.render_widget(
        Paragraph::new(diff_lines(view, theme)).block(diff_block),
        columns[1],
    );
}

fn render_commit_editor(frame: &mut Frame, editor: &CommitEditor, theme: &Theme, area: Rect) {
    let width = 64u16.min(area.width);
    let height = 5u16.min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Commit message   [Enter] commit · [Esc] cancel ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let line = Line::from(vec![
        Span::styled(editor.message.clone(), Style::default().fg(theme.node)),
        Span::styled("█", Style::default().fg(theme.branch_badge)),
    ]);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: false }), inner);
}

fn render_confirm(frame: &mut Frame, confirm: &Confirm, theme: &Theme, area: Rect) {
    let width = (confirm.message.chars().count() as u16 + 4)
        .clamp(20, area.width)
        .min(area.width);
    let height = 3u16.min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warn))
        .title(" Confirm ");
    frame.render_widget(Paragraph::new(confirm.message.clone()).block(block), rect);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

fn relative_time(now: i64, then: i64) -> String {
    let delta = (now - then).max(0);
    match delta {
        d if d < 60 => format!("{d}s"),
        d if d < 3_600 => format!("{}m", d / 60),
        d if d < 86_400 => format!("{}h", d / 3_600),
        d if d < 604_800 => format!("{}d", d / 86_400),
        d if d < 2_629_800 => format!("{}w", d / 604_800),
        d if d < 31_557_600 => format!("{}mo", d / 2_629_800),
        d => format!("{}y", d / 31_557_600),
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let clamped = t.clamp(0.0, 1.0);
    1.0 - (1.0 - clamped).powi(3)
}
