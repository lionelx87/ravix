use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use syntect::parsing::SyntaxReference;

use crate::app::{App, BranchCreate, CommitEditor, Confirm, Panel};
use crate::branches::BranchPanel;
use crate::conflict::{ConflictBrowser, OpKind, Segment, Side};
use crate::enrich::{self, emphasis_added, emphasis_removed, word_diff};
use crate::git::BadgeKind;
use crate::join::JoinMenu;
use crate::stash::StashPanel;
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
    add_bg: Color,
    remove_bg: Color,
    add_emph_bg: Color,
    remove_emph_bg: Color,
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
            add_bg: Color::Rgb(24, 42, 30),
            remove_bg: Color::Rgb(48, 28, 30),
            add_emph_bg: Color::Rgb(44, 82, 52),
            remove_emph_bg: Color::Rgb(96, 42, 46),
        }
    }
}

impl Theme {
    fn lane(&self, lane: usize) -> Color {
        self.lanes[lane % self.lanes.len()]
    }

    fn badge_style(&self, kind: BadgeKind) -> Style {
        let color = match kind {
            BadgeKind::Head | BadgeKind::CurrentBranch => self.head_badge,
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
    if let Some(menu) = app.join_menu() {
        render_ghost_preview(frame, menu, &theme, commit_area);
    }
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
    if let Some(panel) = app.branch_panel() {
        render_branch_panel(frame, panel, &theme, graph_area);
    }
    if let Some(menu) = app.join_menu() {
        render_join_menu(frame, menu, &theme, graph_area);
    }
    if let Some(panel) = app.stash_panel() {
        render_stash_panel(frame, panel, &theme, graph_area);
    }
    if let Some(browser) = app.conflict_browser() {
        render_conflict_browser(frame, browser, &theme, graph_area);
    }
    if let Some(editor) = app.commit_editor() {
        render_commit_editor(frame, editor, &theme, area);
    }
    if let Some(editor) = app.branch_create() {
        render_branch_create(frame, editor, &theme, area);
    }
    if let Some(confirm) = app.confirm() {
        render_confirm(frame, confirm, &theme, area);
    }
    if let Some(message) = app.alert() {
        render_alert(frame, message, &theme, area);
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
    let drag = app.drag();
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
            let flash = app.checkout_flash();
            for badge in commit_badges {
                let text = match badge.kind {
                    BadgeKind::CurrentBranch => format!(" HEAD → {} ", badge.label),
                    _ => format!(" {} ", badge.label),
                };
                let mut style = theme.badge_style(badge.kind);
                if badge.kind == BadgeKind::CurrentBranch && flash > 0.0 {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                spans.push(Span::styled(text, style));
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

        if let Some((source, _, hover)) = drag
            && index == hover
        {
            spans.push(Span::styled(
                format!("   ⟵ drop {source}"),
                Style::default()
                    .fg(theme.head_badge)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        buffer.set_line(area.x, y, &Line::from(spans), area.width);

        let highlight = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };
        if is_selected {
            buffer.set_style(
                highlight,
                Style::default()
                    .bg(theme.selection_bg)
                    .add_modifier(Modifier::BOLD),
            );
        }
        if drag.is_some_and(|(_, source_row, _)| source_row == index) {
            buffer.set_style(
                highlight,
                Style::default()
                    .bg(theme.selection_bg)
                    .add_modifier(Modifier::DIM),
            );
        }
        if drag.is_some_and(|(_, _, hover)| hover == index) {
            buffer.set_style(highlight, Style::default().bg(theme.add_emph_bg));
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
    if let Some((ahead, behind)) = app.head_tracking() {
        spans.push(Span::styled(
            format!("  ↑{ahead} ↓{behind}"),
            Style::default().fg(theme.meta),
        ));
    }
    if let Some((verb, spinner)) = app.remote_status() {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame = FRAMES[(spinner / 4) % FRAMES.len()];
        spans.push(Span::styled(
            format!("  {frame} {verb}…"),
            Style::default()
                .fg(theme.branch_badge)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(notice) = app.notice() {
        spans.push(Span::raw("  "));
        let styled = if app.notice_is_error() {
            Span::styled(
                format!("⚠ {notice}"),
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                notice.to_string(),
                Style::default()
                    .fg(theme.added)
                    .add_modifier(Modifier::BOLD),
            )
        };
        spans.push(styled);
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

fn render_branch_panel(frame: &mut Frame, panel: &BranchPanel, theme: &Theme, area: Rect) {
    let eased = ease_out_cubic(panel.slide);
    let full_width = ((area.width as f32) * 0.4).max(34.0).min(area.width as f32) as u16;
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
        .title(" Branches   [Enter] checkout · [n] new · [d] delete · [M] join ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();
    for (index, entry) in panel.entries.iter().enumerate() {
        let selected = index == panel.selected;
        let mut spans = vec![
            Span::styled(
                if selected { "❯ " } else { "  " },
                Style::default().fg(theme.marker),
            ),
            Span::styled(
                if entry.is_head { "● " } else { "  " },
                Style::default()
                    .fg(theme.head_badge)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                entry.name.clone(),
                Style::default().fg(if selected { theme.node } else { theme.summary }),
            ),
        ];
        if entry.ahead > 0 || entry.behind > 0 {
            spans.push(Span::styled(
                format!("  ↑{} ↓{}", entry.ahead, entry.behind),
                Style::default().fg(theme.meta),
            ));
        }
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "no local branches",
            Style::default().fg(theme.meta),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_ghost_preview(frame: &mut Frame, menu: &JoinMenu, theme: &Theme, area: Rect) {
    if area.height < 2 || !menu.focused().is_some_and(|option| option.enabled) {
        return;
    }
    let ghost = Style::default().fg(theme.meta).add_modifier(Modifier::DIM);
    let node_line = Line::from(vec![
        Span::styled("  ◈", ghost.add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("   preview · {}", menu.summary),
            ghost.add_modifier(Modifier::ITALIC),
        ),
    ]);
    let source = menu.source_name.as_deref().unwrap_or("source");
    let edge_line = Line::from(vec![
        Span::styled(" ╱ ╲", ghost),
        Span::styled(
            format!("  HEAD   {source}"),
            ghost.add_modifier(Modifier::ITALIC),
        ),
    ]);
    let buffer = frame.buffer_mut();
    buffer.set_line(area.x, area.y, &node_line, area.width);
    buffer.set_line(area.x, area.y + 1, &edge_line, area.width);
}

fn render_join_menu(frame: &mut Frame, menu: &JoinMenu, theme: &Theme, area: Rect) {
    let eased = ease_out_cubic(menu.slide);
    let full_width = ((area.width as f32) * 0.45)
        .max(40.0)
        .min(area.width as f32) as u16;
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
        .title(format!(" {} ", menu.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();
    for (index, option) in menu.options.iter().enumerate() {
        let selected = index == menu.selected;
        let label_color = if !option.enabled {
            theme.meta
        } else if selected {
            theme.node
        } else {
            theme.summary
        };
        let mut spans = vec![
            Span::styled(
                if selected { "❯ " } else { "  " },
                Style::default().fg(theme.marker),
            ),
            Span::styled(
                option.label.clone(),
                Style::default()
                    .fg(label_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  [{}]", option.note),
                Style::default().fg(theme.meta),
            ),
        ];
        if !option.enabled {
            spans.push(Span::styled("  blocked", Style::default().fg(theme.warn)));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("⟿ {}", menu.summary),
        Style::default()
            .fg(theme.branch_badge)
            .add_modifier(Modifier::BOLD),
    )));

    if !menu.conflict_files.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Conflicts",
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
        )));
        for file in &menu.conflict_files {
            lines.push(Line::from(Span::styled(
                format!("  {file}"),
                Style::default().fg(theme.removed),
            )));
        }
        lines.push(Line::from(Span::styled(
            "[Enter] proceeds into the conflict browser",
            Style::default().fg(theme.meta),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[Enter] run · [Esc] cancel",
        Style::default().fg(theme.meta),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_stash_panel(frame: &mut Frame, panel: &StashPanel, theme: &Theme, area: Rect) {
    let eased = ease_out_cubic(panel.slide);
    let full_width = ((area.width as f32) * 0.45)
        .max(38.0)
        .min(area.width as f32) as u16;
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
        .title(" Stashes   [p] pop · [a] apply · [d] drop ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();
    for (index, entry) in panel.entries.iter().enumerate() {
        let selected = index == panel.selected;
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "❯ " } else { "  " },
                Style::default().fg(theme.marker),
            ),
            Span::styled(
                format!("stash@{{{}}} ", entry.index),
                Style::default().fg(theme.short_id),
            ),
            Span::styled(
                entry.message.clone(),
                Style::default().fg(if selected { theme.node } else { theme.summary }),
            ),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "no stashes",
            Style::default().fg(theme.meta),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_conflict_browser(
    frame: &mut Frame,
    browser: &ConflictBrowser,
    theme: &Theme,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let columns = Layout::horizontal([Constraint::Length(38), Constraint::Min(0)]).split(area);

    let files_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warn))
        .title(" Conflicts ");
    let mut file_lines = Vec::new();
    for (index, file) in browser.files.iter().enumerate() {
        let selected = index == browser.file;
        let (mark, mark_color) = if file.is_resolved() {
            ("✓", theme.added)
        } else {
            ("◆", theme.warn)
        };
        file_lines.push(Line::from(vec![
            Span::styled(
                if selected { "❯ " } else { "  " },
                Style::default().fg(theme.marker),
            ),
            Span::styled(
                format!("{mark} "),
                Style::default().fg(mark_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                file.path.clone(),
                Style::default().fg(if selected { theme.node } else { theme.summary }),
            ),
            Span::styled(
                format!("  ({} blocks)", file.count()),
                Style::default().fg(theme.meta),
            ),
        ]));
    }
    if file_lines.is_empty() {
        file_lines.push(Line::from(Span::styled(
            "all resolved — press [c] to continue",
            Style::default()
                .fg(theme.added)
                .add_modifier(Modifier::BOLD),
        )));
    }
    frame.render_widget(Paragraph::new(file_lines).block(files_block), columns[0]);

    let title = browser
        .focused_file()
        .map(|file| format!(" {} ", file.path))
        .unwrap_or_else(|| " Conflict ".to_string());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.label))
        .title(title);
    let inner = block.inner(columns[1]);
    frame.render_widget(block, columns[1]);

    let progress = match browser.progress {
        Some((current, total)) => format!(" · step {current}/{total}"),
        None => String::new(),
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!("{} in progress{progress}", browser.op.label()),
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    if let Some(file) = browser.focused_file() {
        let syntax = enrich::highlighter().language(&file.path);
        let mut conflict_index = 0;
        for segment in &file.segments {
            match segment {
                Segment::Context(context) => {
                    for line in context {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            Style::default().fg(theme.meta),
                        )));
                    }
                }
                Segment::Conflict { ours, theirs } => {
                    let focused = conflict_index == browser.block;
                    let choice = file.choices.get(conflict_index).copied().flatten();
                    let taken = match choice {
                        Some(Side::Ours) => "  · took OURS",
                        Some(Side::Theirs) => "  · took THEIRS",
                        None => "",
                    };
                    lines.push(Line::from(Span::styled(
                        format!(
                            "{} conflict {}/{}{taken}",
                            if focused { "❯" } else { " " },
                            conflict_index + 1,
                            file.count(),
                        ),
                        Style::default()
                            .fg(theme.branch_badge)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        "  ── OURS ──",
                        Style::default()
                            .fg(theme.added)
                            .add_modifier(Modifier::BOLD),
                    )));
                    for line in ours {
                        lines.push(conflict_code_line(line, syntax, theme.add_bg, theme));
                    }
                    lines.push(Line::from(Span::styled(
                        "  ── THEIRS ──",
                        Style::default()
                            .fg(theme.removed)
                            .add_modifier(Modifier::BOLD),
                    )));
                    for line in theirs {
                        lines.push(conflict_code_line(line, syntax, theme.remove_bg, theme));
                    }
                    lines.push(Line::from(""));
                    conflict_index += 1;
                }
            }
        }
    }
    let skip = if browser.op == OpKind::Rebase {
        " · [s] skip"
    } else {
        ""
    };
    lines.push(Line::from(Span::styled(
        format!(
            "[o] ours · [t] theirs · [e] edit · [c] continue{skip} · [A]/[Esc] abort — {} left",
            browser.remaining()
        ),
        Style::default().fg(theme.meta),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn conflict_code_line(
    text: &str,
    syntax: Option<&SyntaxReference>,
    bg: Color,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", Style::default().bg(bg))];
    let highlighted: Vec<(Color, String)> = match syntax {
        Some(syntax) => enrich::highlighter()
            .highlight(syntax, text)
            .into_iter()
            .map(|span| {
                (
                    Color::Rgb(span.color.0, span.color.1, span.color.2),
                    span.text,
                )
            })
            .collect(),
        None => vec![(theme.summary, text.to_string())],
    };
    for (fg, chunk) in highlighted {
        spans.push(Span::styled(chunk, Style::default().fg(fg).bg(bg)));
    }
    Line::from(spans)
}

fn render_help(frame: &mut Frame, theme: &Theme, area: Rect) {
    let bindings = [
        ("j / ↓", "select next commit"),
        ("k / ↑", "select previous commit"),
        ("gg / G", "jump to first / last"),
        ("PgUp / PgDn", "page up / down"),
        ("Enter", "open commit detail panel"),
        ("space", "checkout commit / stage file or hunk"),
        ("b", "branches: checkout · n new · d delete"),
        ("M", "join: merge / cherry-pick / rebase (predicted)"),
        ("f / p / P", "fetch / pull / push (background)"),
        ("s / S", "stash changes / stash list"),
        ("drag", "drop a branch onto another to join"),
        ("c", "commit staged changes"),
        ("d", "discard (file or hunk)"),
        ("u", "undo last action"),
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

    let syntax = enrich::highlighter().language(&diff.new_path);
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
        let emphasis = intraline_emphasis(&hunk.lines);
        for (line, flags) in hunk.lines.iter().zip(&emphasis) {
            lines.push(diff_body_line(line, flags, syntax, theme));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn intraline_emphasis(lines: &[String]) -> Vec<Vec<bool>> {
    let mut emphasis: Vec<Vec<bool>> = lines.iter().map(|_| Vec::new()).collect();
    let mut index = 0;
    while index < lines.len() {
        if marker_of(&lines[index]) != '-' {
            index += 1;
            continue;
        }
        let removed_start = index;
        while index < lines.len() && marker_of(&lines[index]) == '-' {
            index += 1;
        }
        let added_start = index;
        while index < lines.len() && marker_of(&lines[index]) == '+' {
            index += 1;
        }
        let pairs = (added_start - removed_start).min(index - added_start);
        for offset in 0..pairs {
            let removed = removed_start + offset;
            let added = added_start + offset;
            let spans = word_diff(content_of(&lines[removed]), content_of(&lines[added]));
            emphasis[removed] = emphasis_removed(&spans);
            emphasis[added] = emphasis_added(&spans);
        }
    }
    emphasis
}

fn diff_body_line(
    line: &str,
    emphasis: &[bool],
    syntax: Option<&SyntaxReference>,
    theme: &Theme,
) -> Line<'static> {
    let marker = marker_of(line);
    let content = content_of(line);
    let (gutter, base_bg, emph_bg) = match marker {
        '+' => (theme.added, Some(theme.add_bg), Some(theme.add_emph_bg)),
        '-' => (
            theme.removed,
            Some(theme.remove_bg),
            Some(theme.remove_emph_bg),
        ),
        _ => (theme.meta, None, None),
    };

    let highlighted: Vec<(Color, String)> = match syntax {
        Some(syntax) => enrich::highlighter()
            .highlight(syntax, content)
            .into_iter()
            .map(|span| {
                (
                    Color::Rgb(span.color.0, span.color.1, span.color.2),
                    span.text,
                )
            })
            .collect(),
        None => vec![(theme.summary, content.to_string())],
    };

    let mut spans = vec![Span::styled(
        format!("{marker} "),
        Style::default().fg(gutter).add_modifier(Modifier::BOLD),
    )];

    let mut char_index = 0;
    for (fg, text) in highlighted {
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let emphasized = emphasis.get(char_index + start).copied().unwrap_or(false);
            let mut end = start;
            while end < chars.len()
                && emphasis.get(char_index + end).copied().unwrap_or(false) == emphasized
            {
                end += 1;
            }
            let mut style = Style::default().fg(fg);
            if let Some(bg) = if emphasized { emph_bg } else { base_bg } {
                style = style.bg(bg);
            }
            spans.push(Span::styled(
                chars[start..end].iter().collect::<String>(),
                style,
            ));
            start = end;
        }
        char_index += chars.len();
    }

    Line::from(spans)
}

fn marker_of(line: &str) -> char {
    line.chars().next().unwrap_or(' ')
}

fn content_of(line: &str) -> &str {
    let marker = marker_of(line);
    &line[marker.len_utf8().min(line.len())..]
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

fn render_branch_create(frame: &mut Frame, editor: &BranchCreate, theme: &Theme, area: Rect) {
    let width = 48u16.min(area.width);
    let height = 3u16.min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" New branch   [Enter] create · [Esc] cancel ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let line = Line::from(vec![
        Span::styled(editor.name.clone(), Style::default().fg(theme.node)),
        Span::styled("█", Style::default().fg(theme.branch_badge)),
    ]);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: false }), inner);
}

fn render_alert(frame: &mut Frame, message: &str, theme: &Theme, area: Rect) {
    let width = (message.chars().count() as u16 + 4).clamp(24, area.width);
    let height = 4u16.min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warn))
        .title(" ⚠ Blocked ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let lines = vec![
        Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(theme.node).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "press any key to dismiss",
            Style::default().fg(theme.meta),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
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
