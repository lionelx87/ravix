use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use std::collections::HashMap;

use git2::Oid;
use syntect::parsing::SyntaxReference;

use crate::app::{
    App, BranchCreate, CommitEditor, Confirm, FocusPanel, InputContext, NavDirection, Palette,
    Panel, PasswordPrompt, SubmodulePanel,
};
use crate::branches::BranchPanel;
use crate::conflict::{ConflictBrowser, OpKind, Segment, Side};
use crate::enrich::{self, emphasis_added, emphasis_removed, word_diff};
use crate::git::{BadgeKind, CONFLICTED, FileStatus, LineStats, RefBadge};
use crate::help::context_help;
use crate::join::JoinMenu;
use crate::slide::SlidePanel;
use crate::staging::{FileDiff, content_of, marker_of, split_rows};
use crate::stash::{Focus as StashFocus, StashEntry, StashMessage, StashView};
use crate::submodule::{Submodule, SyncState};
use crate::visibility::Visibility;
use crate::working::{Focus, WorkingView};

mod graph_view;

use graph_view::display_width;

const SELECTION_MARKER: &str = "❯ ";
const CARD_ROWS: usize = 2;
const PANEL_CARD_ROWS: usize = 8;
const SPLIT_SEPARATOR: &str = " │ ";

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
    let area = frame.area();
    let Some(mut transition) = app.take_nav_transition() else {
        render_app(frame, app, now, area);
        return;
    };

    let eased = ease_out_cubic(transition.progress.clamp(0.0, 1.0));
    let fraction = match transition.direction {
        NavDirection::Push => eased,
        NavDirection::Pop => 1.0 - eased,
    };
    let overlay = overlay_rect(area, fraction);
    match transition.direction {
        NavDirection::Push => {
            render_app(frame, &mut transition.outgoing, now, area);
            if let Some(rect) = overlay {
                frame.render_widget(Clear, rect);
                render_app(frame, app, now, rect);
            }
        }
        NavDirection::Pop => {
            render_app(frame, app, now, area);
            if let Some(rect) = overlay {
                frame.render_widget(Clear, rect);
                render_app(frame, &mut transition.outgoing, now, rect);
            }
        }
    }
    app.restore_nav_transition(transition);
}

fn overlay_rect(area: Rect, fraction: f32) -> Option<Rect> {
    let width = (f32::from(area.width) * fraction.clamp(0.0, 1.0)).round() as u16;
    if width == 0 {
        return None;
    }
    Some(Rect {
        x: area.right() - width,
        width,
        ..area
    })
}

fn render_app(frame: &mut Frame, app: &mut App, now: i64, area: Rect) {
    let theme = Theme::default();
    let (mut graph_area, status_area) = regions(area);

    if let Some(path) = app.breadcrumb()
        && graph_area.height > 1
    {
        let bar_area = Rect {
            height: 1,
            ..graph_area
        };
        graph_area = Rect {
            y: graph_area.y + 1,
            height: graph_area.height - 1,
            ..graph_area
        };
        render_submodule_bar(frame, app, &path, &theme, bar_area);
    }

    let wip_rows = u16::from(app.has_wip());
    let commit_area = Rect {
        x: graph_area.x,
        y: graph_area.y + wip_rows,
        width: graph_area.width,
        height: graph_area.height.saturating_sub(wip_rows),
    };
    app.set_viewport(commit_area.height.saturating_sub(1) as usize);

    render_graph(frame, app, &theme, commit_area, now);
    if let Some(menu) = app.join_menu() {
        let rows_area = Rect {
            x: commit_area.x,
            y: commit_area.y + 1,
            width: commit_area.width,
            height: commit_area.height.saturating_sub(1),
        };
        render_ghost_preview(frame, menu, &theme, rows_area);
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

    match app.working().map(|view| view.fullscreen) {
        Some(true) => render_working_fullscreen(frame, app, &theme, graph_area),
        Some(false) => render_working_panel(frame, app, &theme, graph_area),
        None => {}
    }
    match app.panel().map(|panel| panel.fullscreen) {
        Some(true) => render_commit_fullscreen(frame, app, &theme, graph_area),
        Some(false) => {
            let panel = app.panel().unwrap();
            render_panel(frame, app, panel, &theme, graph_area);
        }
        None => {}
    }
    if let Some(panel) = app.branch_panel() {
        render_branch_panel(
            frame,
            panel,
            app.visibility(),
            app.branch_filter_query(),
            app.branch_filter_editing(),
            &theme,
            graph_area,
        );
    }
    if let Some(menu) = app.join_menu() {
        render_join_menu(frame, menu, &theme, graph_area);
    }
    match app.stash_view().map(|view| view.fullscreen) {
        Some(true) => render_stash_fullscreen(frame, app, &theme, graph_area, now),
        Some(false) => render_stash_panel(frame, app, &theme, graph_area, now),
        None => {}
    }
    if let Some(panel) = app.focus_panel() {
        render_focus_panel(frame, panel, &theme, graph_area);
    }
    if let Some(panel) = app.submodule_panel() {
        render_submodule_panel(frame, panel, app.breadcrumb().is_some(), &theme, graph_area);
    }
    if let Some(browser) = app.conflict_browser() {
        render_conflict_browser(frame, browser, &theme, graph_area);
    }
    if let Some(editor) = app.commit_editor() {
        render_commit_editor(frame, editor, &theme, area);
    }
    if let Some(message) = app.stash_message() {
        render_stash_message(frame, message, &theme, area);
    }
    if let Some(editor) = app.branch_create() {
        render_branch_create(frame, editor, &theme, area);
    }
    if let Some(name) = app.focus_name() {
        render_focus_name(frame, name, &theme, area);
    }
    if let Some(confirm) = app.confirm() {
        render_confirm(frame, confirm, &theme, area);
    }
    if let Some(palette) = app.palette() {
        render_palette(frame, palette, &theme, area);
    }
    if let Some(message) = app.alert() {
        render_alert(frame, message, &theme, area);
    }
    if app.help_visible() {
        render_help(frame, app.input_context(), &theme, area);
    }
    if let Some(prompt) = app.password_prompt() {
        render_password_prompt(frame, prompt, &theme, area);
    }
}

fn lane_color(key: &Oid, badges: &HashMap<Oid, Vec<RefBadge>>, theme: &Theme) -> Color {
    let label = branch_name_at(key, badges).unwrap_or_else(|| key.to_string());
    theme.lane(fnv1a(&label) as usize)
}

fn branch_name_at(oid: &Oid, badges: &HashMap<Oid, Vec<RefBadge>>) -> Option<String> {
    badges
        .get(oid)?
        .iter()
        .find(|badge| {
            matches!(
                badge.kind,
                BadgeKind::LocalBranch | BadgeKind::CurrentBranch
            )
        })
        .map(|badge| badge.label.clone())
}

fn status_color(code: char, theme: &Theme) -> Color {
    match code {
        'A' => theme.added,
        'D' => theme.removed,
        'M' => theme.wip,
        'R' | 'C' | 'T' => theme.branch_badge,
        CONFLICTED => theme.warn,
        _ => theme.meta,
    }
}

fn status_span(status: FileStatus, theme: &Theme) -> Span<'static> {
    let code = status.code();
    Span::styled(
        format!("{code} "),
        Style::default()
            .fg(status_color(code, theme))
            .add_modifier(Modifier::BOLD),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_diff_pane(
    frame: &mut Frame,
    diff: Option<&FileDiff>,
    split: bool,
    focused_hunk: Option<usize>,
    scroll: &mut u16,
    hscroll: &mut u16,
    snap_to_focus: bool,
    focused: bool,
    theme: &Theme,
    area: Rect,
) {
    let mode = if split {
        "side-by-side · [v] unified"
    } else {
        "[v] side-by-side"
    };
    let title = diff
        .map(|diff| format!(" {}   {mode} ", diff.new_path))
        .unwrap_or_else(|| format!(" Diff   {mode} "));
    let border = if focused { theme.marker } else { theme.label };
    let diff_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(title);
    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;

    if split {
        let col_width = inner_width.saturating_sub(SPLIT_SEPARATOR.chars().count()) / 2;
        let overflow = diff
            .map(max_content_width)
            .unwrap_or(0)
            .saturating_sub(col_width) as u16;
        *hscroll = (*hscroll).min(overflow);
    } else {
        *hscroll = 0;
    }
    let offset = *hscroll as usize;
    let window = |scroll: u16| scroll as usize..scroll as usize + inner_height;
    let (mut content, focus_offset) = diff_pane_lines(
        diff,
        split,
        focused_hunk,
        inner_width,
        window(*scroll),
        offset,
        theme,
    );

    let mut target = *scroll;
    if let Some(offset) = focus_offset.filter(|_| snap_to_focus) {
        let offset = offset as u16;
        if !(target..target.saturating_add(inner_height as u16)).contains(&offset) {
            target = offset;
        }
    }
    let max_scroll = content.len().saturating_sub(inner_height) as u16;
    target = target.min(max_scroll);
    if target != *scroll {
        *scroll = target;
        content = diff_pane_lines(
            diff,
            split,
            focused_hunk,
            inner_width,
            window(target),
            offset,
            theme,
        )
        .0;
    }
    frame.render_widget(
        Paragraph::new(content)
            .block(diff_block)
            .scroll((*scroll, 0)),
        area,
    );
}

fn fnv1a(text: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

fn sparkle_burst(progress: f32) -> &'static str {
    if progress > 0.66 {
        "✦ ✧ ✨ "
    } else if progress > 0.33 {
        "✧ ✨ "
    } else {
        "✨ "
    }
}

fn render_graph(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect, now: i64) {
    if area.height == 0 {
        return;
    }
    if app.commits().is_empty() {
        let empty = Paragraph::new("No commits to show").style(Style::default().fg(theme.meta));
        frame.render_widget(empty, area);
        return;
    }
    graph_view::render(frame, app, theme, area, now);
}

fn render_submodule_bar(frame: &mut Frame, app: &App, path: &str, theme: &Theme, area: Rect) {
    let branch = app
        .meta()
        .head_branch
        .clone()
        .unwrap_or_else(|| "detached".to_string());
    let mut spans = vec![
        Span::styled(" ⌂ ", Style::default().fg(theme.marker)),
        Span::styled(
            path.to_string(),
            Style::default().fg(theme.node).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  ⎇ {branch}"),
            Style::default().fg(theme.head_badge),
        ),
    ];
    if let Some(short_id) = app.head_short_id() {
        spans.push(Span::styled(
            format!(" @{short_id}"),
            Style::default().fg(theme.short_id),
        ));
    }
    match app.parent_sync() {
        Some(SyncState::Drifted) => {
            spans.push(Span::styled("  ◆ drifted", Style::default().fg(theme.warn)))
        }
        Some(SyncState::Synced) => spans.push(Span::styled(
            "  ● in sync",
            Style::default().fg(theme.added),
        )),
        _ => {}
    }
    let status = app.status();
    let modified = status.staged.len() + status.unstaged.len();
    if modified > 0 {
        spans.push(Span::styled(
            format!("  ±{modified}"),
            Style::default().fg(theme.wip),
        ));
    }
    if !status.untracked.is_empty() {
        spans.push(Span::styled(
            format!("  ?{}", status.untracked.len()),
            Style::default().fg(theme.wip),
        ));
    }
    spans.push(Span::styled(
        "   < / Esc back to parent",
        Style::default().fg(theme.meta),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.status_bg)),
        area,
    );
}

fn render_status(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let meta = app.meta();
    let branch = meta.head_branch.as_deref().unwrap_or("detached HEAD");
    let position = format!("{}/{}", app.selected() + 1, app.commits().len().max(1));

    let heading = meta.name.clone();
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(heading, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  ⎇ "),
    ];
    spans.extend([
        Span::styled(branch.to_string(), Style::default().fg(theme.branch_badge)),
        Span::raw("  "),
        Span::raw(position),
    ]);
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
        lines.push(Line::from(vec![
            Span::raw(" "),
            status_span(file.status, theme),
            Span::raw(file.path.clone()),
        ]));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn slide_rect(area: Rect, slide: f32, fraction: f32, min_width: f32) -> Option<Rect> {
    let eased = ease_out_cubic(slide);
    let full_width = ((area.width as f32) * fraction)
        .max(min_width)
        .min(area.width as f32) as u16;
    let visible = ((full_width as f32) * eased).round() as u16;
    if visible < 6 {
        return None;
    }
    Some(Rect {
        x: area.right() - visible,
        y: area.y,
        width: visible,
        height: area.height,
    })
}

struct SlideList<'a> {
    title: &'a str,
    empty: &'a str,
    hints: &'a [&'a str],
    fraction: f32,
    min_width: f32,
}

fn render_slide_list<T>(
    frame: &mut Frame,
    panel: &SlidePanel<T>,
    theme: &Theme,
    area: Rect,
    list: SlideList,
    mut row: impl FnMut(&T, bool) -> Line<'static>,
) {
    let Some(rect) = slide_rect(area, panel.slide, list.fraction, list.min_width) else {
        return;
    };
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(list.title.to_string());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let footer_height = list.hints.len() as u16;
    let (rows_area, footer_area) = if footer_height > 0 && inner.height > footer_height + 2 {
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(footer_height + 1)])
            .split(inner);
        (chunks[0], Some(chunks[1]))
    } else {
        (inner, None)
    };

    let visible = rows_area.height as usize;
    let first = panel
        .selected
        .saturating_sub(visible / 2)
        .min(panel.entries.len().saturating_sub(visible));
    let mut lines: Vec<Line> = panel
        .entries
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(index, entry)| row(entry, index == panel.selected))
        .collect();
    if panel.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            list.empty,
            Style::default().fg(theme.meta),
        )));
    }
    frame.render_widget(Paragraph::new(lines), rows_area);

    if let Some(footer_area) = footer_area {
        let separator = "─".repeat(footer_area.width as usize);
        let mut hint_lines = vec![Line::from(Span::styled(
            separator,
            Style::default().fg(theme.panel_border),
        ))];
        for hint in list.hints {
            hint_lines.push(Line::from(Span::styled(
                *hint,
                Style::default().fg(theme.meta),
            )));
        }
        frame.render_widget(Paragraph::new(hint_lines), footer_area);
    }
}

fn render_branch_panel(
    frame: &mut Frame,
    panel: &BranchPanel,
    visibility: &Visibility,
    filter: Option<&str>,
    filter_editing: bool,
    theme: &Theme,
    area: Rect,
) {
    let title = match filter {
        Some(query) if filter_editing => format!(" Branches   /{query}▏"),
        Some(query) => format!(" Branches   /{query} "),
        None => " Branches ".to_string(),
    };
    let hints: &[&str] = if filter_editing {
        &[
            "↑/↓ move · ↵ apply filter (keys act on matches)",
            "Esc cancel filter",
        ]
    } else {
        &[
            "↵ checkout · n new · d delete · M join",
            "Space hide · o solo/all · p pin · / filter",
        ]
    };
    render_slide_list(
        frame,
        panel,
        theme,
        area,
        SlideList {
            title: &title,
            empty: "no matching branches",
            hints,
            fraction: 0.4,
            min_width: 40.0,
        },
        |entry, selected| {
            let hidden = !visibility.is_visible(&entry.name, entry.is_head);
            let pinned = visibility.is_pinned(&entry.name);
            let (mark, mark_color) = if pinned {
                ("★ ", theme.branch_badge)
            } else if hidden {
                ("◌ ", theme.meta)
            } else {
                ("  ", theme.marker)
            };
            let is_remote = entry.remote.is_some();
            let base_color = if selected {
                theme.node
            } else if is_remote {
                theme.meta
            } else {
                theme.summary
            };
            let mut name_style = Style::default().fg(base_color);
            if is_remote {
                name_style = name_style.add_modifier(Modifier::ITALIC);
            }
            if hidden {
                name_style = name_style.add_modifier(Modifier::DIM);
            }
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
                Span::styled(mark, Style::default().fg(mark_color)),
                Span::styled(entry.name.clone(), name_style),
            ];
            if entry.ahead > 0 || entry.behind > 0 {
                spans.push(Span::styled(
                    format!("  ↑{} ↓{}", entry.ahead, entry.behind),
                    Style::default().fg(theme.meta),
                ));
            }
            Line::from(spans)
        },
    );
}

fn render_ghost_preview(frame: &mut Frame, menu: &JoinMenu, theme: &Theme, area: Rect) {
    if area.height < 2 || !menu.panel.focused().is_some_and(|option| option.enabled) {
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
    let Some(rect) = slide_rect(area, menu.panel.slide, 0.45, 40.0) else {
        return;
    };
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(format!(" {} ", menu.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();
    for (index, option) in menu.panel.entries.iter().enumerate() {
        let selected = index == menu.panel.selected;
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

fn stash_origin(entry: &StashEntry) -> String {
    match (&entry.branch, entry.message.base_id()) {
        (Some(branch), _) => branch.clone(),
        (None, Some(base)) => base.to_string(),
        (None, None) => "detached".to_string(),
    }
}

fn file_word(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

fn padded(spans: Vec<Span<'static>>, width: usize, style: Style) -> Line<'static> {
    let used: usize = spans.iter().map(|span| display_width(&span.content)).sum();
    let mut spans = spans;
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }
    Line::from(spans).style(style)
}

fn stats_spans(stats: LineStats, theme: &Theme) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!("+{} ", stats.added),
            Style::default().fg(theme.added),
        ),
        Span::styled(
            format!("−{}", stats.removed),
            Style::default().fg(theme.removed),
        ),
    ]
}

fn right_aligned(
    mut spans: Vec<Span<'static>>,
    tail: Vec<Span<'static>>,
    width: usize,
) -> Vec<Span<'static>> {
    let used: usize = spans.iter().map(|span| display_width(&span.content)).sum();
    let tail_width: usize = tail.iter().map(|span| display_width(&span.content)).sum();
    if used + tail_width + 2 <= width {
        spans.push(Span::raw(" ".repeat(width - used - tail_width)));
        spans.extend(tail);
    }
    spans
}

fn card_window(view: &StashView, rows: usize) -> std::ops::Range<usize> {
    let total = view.panel.entries.len();
    let visible = (rows / CARD_ROWS).clamp(1, total.max(1));
    let first = view
        .panel
        .selected
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible));
    first..(first + visible).min(total)
}

fn stash_card_lines(
    app: &App,
    view: &StashView,
    width: usize,
    theme: &Theme,
    now: i64,
    rows: usize,
) -> Vec<Line<'static>> {
    if view.panel.entries.is_empty() {
        return vec![Line::from(Span::styled(
            "no stashes — s stashes your changes",
            Style::default().fg(theme.meta),
        ))];
    }

    let window = card_window(view, rows);
    let mut lines = Vec::new();
    for (index, entry) in view
        .panel
        .entries
        .iter()
        .enumerate()
        .skip(window.start)
        .take(window.len())
    {
        let selected = index == view.panel.selected;
        let focused = selected && view.focus == StashFocus::Entries;
        let row = if selected {
            Style::default().bg(theme.selection_bg)
        } else {
            Style::default()
        };
        let named = matches!(entry.message, StashMessage::Named(_));
        let mut message = Style::default().fg(if named { theme.node } else { theme.meta });
        if selected {
            message = message.add_modifier(Modifier::BOLD);
        }
        if !named {
            message = message.add_modifier(Modifier::ITALIC);
        }
        lines.push(padded(
            vec![
                Span::styled(
                    if focused { SELECTION_MARKER } else { "  " },
                    Style::default().fg(theme.marker),
                ),
                Span::styled(
                    format!("{} {}  ", if selected { "▣" } else { "▢" }, entry.index),
                    Style::default()
                        .fg(theme.short_id)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(entry.message.text().to_string(), message),
            ],
            width,
            row,
        ));

        let stats = app.stash_stats(entry);
        let mut meta = vec![
            Span::raw("      "),
            Span::styled("⎇ ", Style::default().fg(theme.meta)),
            Span::styled(stash_origin(entry), Style::default().fg(theme.head_badge)),
        ];
        if let Some(base) = entry.branch.as_ref().and(entry.message.base_id()) {
            meta.push(Span::styled(
                format!(" · {base}"),
                Style::default().fg(theme.short_id),
            ));
        }
        meta.push(Span::styled(
            format!(
                " · {} · {}",
                relative_time(now, entry.time),
                file_word(stats.files)
            ),
            Style::default().fg(theme.meta),
        ));
        let meta = right_aligned(meta, stats_spans(stats.lines, theme), width);
        lines.push(padded(meta, width, row));
    }
    lines
}

fn stash_focus_line(view: &StashView, theme: &Theme) -> Line<'static> {
    let label = match view.focus {
        StashFocus::Entries => "stashes ",
        StashFocus::Files => "files ",
        StashFocus::Hunks => "diff ",
    };
    let keys = match view.focus {
        StashFocus::Entries => "[p] pop  [a] apply  [b] branch  [d] drop",
        StashFocus::Files => "[x] restore one  [p] pop  [a] apply  [d] drop",
        StashFocus::Hunks => "[v] side-by-side  [p] pop  [a] apply",
    };
    Line::from(vec![
        Span::styled(
            label,
            Style::default()
                .fg(theme.branch_badge)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(keys, Style::default().fg(theme.meta)),
    ])
}

fn stash_keys_line(keys: &'static str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(keys, Style::default().fg(theme.meta)))
}

fn stash_file_lines(
    view: &StashView,
    width: usize,
    theme: &Theme,
    heading: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if heading {
        lines.push(section_title("Files", view.files.len(), theme));
    }
    if view.files.is_empty() {
        lines.push(Line::from(Span::styled(
            "  nothing in this entry",
            Style::default().fg(theme.meta),
        )));
        return lines;
    }
    for (index, file) in view.files.iter().enumerate() {
        let selected = index == view.file;
        let focused = selected && view.focus != StashFocus::Entries;
        let row = if focused {
            Style::default().bg(theme.selection_bg)
        } else {
            Style::default()
        };
        let spans = vec![
            Span::styled(
                if focused { SELECTION_MARKER } else { "  " },
                Style::default().fg(theme.marker),
            ),
            status_span(file.status, theme),
            Span::styled(
                file.path.clone(),
                Style::default().fg(if selected { theme.node } else { theme.summary }),
            ),
        ];
        let spans = right_aligned(spans, stats_spans(file.stats, theme), width);
        lines.push(padded(spans, width, row));
    }
    lines
}

fn stash_filter_label(app: &App) -> Option<String> {
    let query = app.stash_filter_query()?;
    Some(if app.stash_filter_editing() {
        format!("/{query}▏")
    } else {
        format!("/{query} ")
    })
}

fn render_stash_panel(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect, now: i64) {
    let Some(rect) = app
        .stash_view()
        .and_then(|view| slide_rect(area, view.panel.slide, 0.5, 46.0))
    else {
        return;
    };
    frame.render_widget(Clear, rect);

    let count = app.stash_view().map_or(0, |view| view.panel.entries.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(match stash_filter_label(app) {
            Some(label) => format!(" Stashes   {count}   {label}"),
            None => format!(" Stashes   {count}   [Enter] fullscreen "),
        });
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let width = inner.width as usize;

    const FOOTER: u16 = 3;
    let (content, footer) = if inner.height > FOOTER + 4 {
        let chunks =
            Layout::vertical([Constraint::Min(0), Constraint::Length(FOOTER)]).split(inner);
        (chunks[0], Some(chunks[1]))
    } else {
        (inner, None)
    };

    let scroll = {
        let view = app.stash_view().unwrap();
        let mut lines = stash_card_lines(app, view, width, theme, now, PANEL_CARD_ROWS);
        lines.push(Line::from(""));
        lines.extend(stash_file_lines(view, width, theme, true));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("── diff {}", "─".repeat(width.saturating_sub(8))),
            Style::default().fg(theme.meta),
        )));
        let focused_hunk = (view.focus == StashFocus::Hunks).then_some(view.hunk);
        let top = view.diff_scroll as usize;
        let window = top..top + content.height as usize;
        match view.diff.as_ref() {
            Some(diff) => lines.extend(diff_lines(diff, focused_hunk, width, window, theme).0),
            None => lines.extend(no_stash_diff_lines(theme)),
        }

        let max_scroll = lines.len().saturating_sub(content.height as usize) as u16;
        let scroll = view.diff_scroll.min(max_scroll);
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), content);

        if let Some(footer) = footer {
            let mut hints = vec![Line::from(Span::styled(
                "─".repeat(width),
                Style::default().fg(theme.panel_border),
            ))];
            hints.push(stash_focus_line(view, theme));
            hints.push(stash_keys_line(
                "[Tab] focus  [/] filter  [Enter] fullscreen  [?] help",
                theme,
            ));
            frame.render_widget(Paragraph::new(hints), footer);
        }
        scroll
    };

    if let Some(view) = app.stash_view_mut() {
        view.diff_scroll = scroll;
    }
}

fn render_stash_fullscreen(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect, now: i64) {
    frame.render_widget(Clear, area);
    let columns = Layout::horizontal([Constraint::Length(46), Constraint::Min(0)]).split(area);

    let (focus, count, view_file_count) = {
        let view = app.stash_view().unwrap();
        (view.focus, view.panel.entries.len(), view.files.len())
    };
    let entry_rows = (count as u16 * CARD_ROWS as u16 + 2)
        .min(area.height / 2)
        .max(3);
    let rows = Layout::vertical([
        Constraint::Length(entry_rows),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(columns[0]);

    let entries_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focus == StashFocus::Entries {
            theme.marker
        } else {
            theme.label
        }))
        .title(match stash_filter_label(app) {
            Some(label) => format!(" Stashes {count} · {label}"),
            None => format!(" Stashes {count} · p pop · a apply · d drop "),
        });
    let files_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focus == StashFocus::Files {
            theme.marker
        } else {
            theme.label
        }))
        .title(format!(
            " Files {} · x restore one · b branch ",
            view_file_count
        ));

    let (entry_lines, file_lines) = {
        let view = app.stash_view().unwrap();
        let width = rows[0].width.saturating_sub(2) as usize;
        (
            stash_card_lines(
                app,
                view,
                width,
                theme,
                now,
                rows[0].height.saturating_sub(2) as usize,
            ),
            stash_file_lines(view, width, theme, false),
        )
    };
    frame.render_widget(Paragraph::new(entry_lines).block(entries_block), rows[0]);

    let list_height = rows[1].height.saturating_sub(2) as usize;
    let file_count = file_lines.len();
    let view = app.stash_view_mut().unwrap();
    view.files_scroll = scroll_into_view(view.files_scroll, view.file, list_height, file_count);
    let files_scroll = view.files_scroll;
    frame.render_widget(
        Paragraph::new(file_lines)
            .block(files_block)
            .scroll((files_scroll, 0)),
        rows[1],
    );

    let hints = {
        let view = app.stash_view().unwrap();
        vec![
            stash_focus_line(view, theme),
            stash_keys_line("[Tab] focus  [v] side-by-side  [Esc] panel", theme),
        ]
    };
    frame.render_widget(Paragraph::new(hints), rows[2]);

    let view = app.stash_view_mut().unwrap();
    let focused_hunk = (view.focus == StashFocus::Hunks).then_some(view.hunk);
    let snap = view.hunk_snap;
    view.hunk_snap = false;
    render_diff_pane(
        frame,
        view.diff.as_ref(),
        view.split,
        focused_hunk,
        &mut view.diff_scroll,
        &mut view.diff_hscroll,
        snap,
        focus == StashFocus::Hunks,
        theme,
        columns[1],
    );
}

fn no_stash_diff_lines(theme: &Theme) -> Vec<Line<'static>> {
    vec![Line::from(Span::styled(
        "  no textual diff",
        Style::default().fg(theme.meta),
    ))]
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
        let verb = if browser.op == OpKind::Stash {
            "finish"
        } else {
            "continue"
        };
        file_lines.push(Line::from(Span::styled(
            format!("all resolved — press [c] to {verb}"),
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
    let headline = if browser.op == OpKind::Stash {
        "stash conflicts — nothing to commit".to_string()
    } else {
        format!("{} in progress{progress}", browser.op.label())
    };
    let mut lines = vec![
        Line::from(Span::styled(
            headline,
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
    let (finish, undo) = if browser.op == OpKind::Stash {
        ("finish", "discard")
    } else {
        ("continue", "abort")
    };
    lines.push(Line::from(Span::styled(
        format!(
            "[o] ours · [t] theirs · [e] edit · [c] {finish}{skip} · [A]/[Esc] {undo} — {} left",
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

fn render_help(frame: &mut Frame, context: InputContext, theme: &Theme, area: Rect) {
    let page = context_help(context);
    let universal = ("Esc / ? / q", "close · help · quit");

    let help_line = |keys: &str, description: &str| {
        Line::from(vec![
            Span::styled(
                format!(" {keys:<12}"),
                Style::default()
                    .fg(theme.branch_badge)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(description.to_string()),
        ])
    };

    let mut lines: Vec<Line> = page
        .bindings
        .iter()
        .map(|(keys, description)| help_line(keys, description))
        .collect();
    lines.push(Line::from(""));
    lines.push(help_line(universal.0, universal.1));

    let content_width = page
        .bindings
        .iter()
        .chain(std::iter::once(&universal))
        .map(|(keys, description)| 1 + keys.chars().count().max(12) + description.chars().count())
        .max()
        .unwrap_or(44);
    let width = (content_width as u16 + 3).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(format!(" Help — {} ", page.title));
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

fn working_file_lines(
    app: &App,
    view: &WorkingView,
    theme: &Theme,
) -> (Vec<Line<'static>>, Option<usize>) {
    let status = app.status();
    let sections = [
        ("Unstaged", &status.unstaged, theme.removed),
        ("Staged", &status.staged, theme.added),
        ("Untracked", &status.untracked, theme.untracked),
    ];

    let mut lines = Vec::new();
    let mut selected_row = None;
    let mut index = 0usize;
    for (title, files, color) in sections {
        if files.is_empty() {
            continue;
        }
        lines.push(section_title(title, files.len(), theme));
        for file in files {
            let selected = index == view.selected;
            if selected {
                selected_row = Some(lines.len());
            }
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
    (lines, selected_row)
}

fn no_diff_lines(theme: &Theme) -> Vec<Line<'static>> {
    vec![Line::from(Span::styled(
        "No textual diff — press Tab to stage hunks",
        Style::default().fg(theme.meta),
    ))]
}

fn hunk_header_line(header: &str, focused: bool, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            if focused { "❯ " } else { "  " },
            Style::default().fg(theme.marker),
        ),
        Span::styled(
            header.to_string(),
            Style::default()
                .fg(theme.branch_badge)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn diff_lines(
    diff: &FileDiff,
    focused_hunk: Option<usize>,
    width: usize,
    visible: std::ops::Range<usize>,
    theme: &Theme,
) -> (Vec<Line<'static>>, Option<usize>) {
    let syntax = enrich::highlighter().language(&diff.new_path);
    let mut lines = Vec::new();
    let mut focus_offset = None;
    for (index, hunk) in diff.hunks.iter().enumerate() {
        let focused = focused_hunk == Some(index);
        if focused {
            focus_offset = Some(lines.len());
        }
        lines.push(hunk_header_line(&hunk.header, focused, theme));
        let emphasis = intraline_emphasis(&hunk.lines);
        for (line, flags) in hunk.lines.iter().zip(&emphasis) {
            let pos = lines.len();
            let (gutter, plain) = diff_line_parts(line, flags, syntax, false, theme);
            let rows = wrap_diff_body(gutter, plain, width);
            let on_screen = pos < visible.end && pos + rows.len() > visible.start;
            if on_screen && syntax.is_some() {
                let (gutter, content) = diff_line_parts(line, flags, syntax, true, theme);
                lines.extend(wrap_diff_body(gutter, content, width));
            } else {
                lines.extend(rows);
            }
        }
        lines.push(Line::from(""));
    }
    (lines, focus_offset)
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

fn diff_line_parts(
    line: &str,
    emphasis: &[bool],
    syntax: Option<&SyntaxReference>,
    highlight: bool,
    theme: &Theme,
) -> (Span<'static>, Vec<Span<'static>>) {
    let marker = marker_of(line);
    let content = content_of(line);
    let (gutter_color, base_bg, emph_bg) = match marker {
        '+' => (theme.added, Some(theme.add_bg), Some(theme.add_emph_bg)),
        '-' => (
            theme.removed,
            Some(theme.remove_bg),
            Some(theme.remove_emph_bg),
        ),
        _ => (theme.meta, None, None),
    };

    let syntax = if highlight { syntax } else { None };
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

    let gutter = Span::styled(
        format!("{marker} "),
        Style::default()
            .fg(gutter_color)
            .add_modifier(Modifier::BOLD),
    );

    let mut spans = Vec::new();
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

    (gutter, spans)
}

fn diff_body_line(
    line: &str,
    emphasis: &[bool],
    syntax: Option<&SyntaxReference>,
    theme: &Theme,
) -> Line<'static> {
    let (gutter, content) = diff_line_parts(line, emphasis, syntax, true, theme);
    let mut spans = vec![gutter];
    spans.extend(content);
    Line::from(spans)
}

fn wrap_diff_body(
    gutter: Span<'static>,
    content: Vec<Span<'static>>,
    width: usize,
) -> Vec<Line<'static>> {
    let gutter_width = gutter.content.chars().count();
    let content_width = width.saturating_sub(gutter_width).max(1);
    let rows = wrap_content_rows(&content, content_width);
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut spans = Vec::with_capacity(row.len() + 1);
            if index == 0 {
                spans.push(gutter.clone());
            } else {
                spans.push(Span::raw(" ".repeat(gutter_width)));
            }
            spans.extend(row);
            Line::from(spans)
        })
        .collect()
}

fn wrap_content_rows(spans: &[Span<'static>], width: usize) -> Vec<Vec<Span<'static>>> {
    let width = width.max(1);
    let chars: Vec<(char, Style)> = spans
        .iter()
        .flat_map(|span| span.content.chars().map(move |ch| (ch, span.style)))
        .collect();
    if chars.is_empty() {
        return vec![Vec::new()];
    }

    let mut rows = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let hard_end = (start + width).min(chars.len());
        let (row_end, next_start) = if hard_end == chars.len() {
            (hard_end, hard_end)
        } else {
            let break_at = (start..=hard_end).rev().find(|&i| chars[i].0 == ' ');
            match break_at {
                Some(i) if i > start => (i, i + 1),
                _ => (hard_end, hard_end),
            }
        };
        rows.push(row_to_spans(&chars[start..row_end]));
        start = next_start.max(start + 1);
    }
    rows
}

fn row_to_spans(row: &[(char, Style)]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut current_style: Option<Style> = None;
    for &(ch, style) in row {
        if current_style == Some(style) {
            current.push(ch);
        } else {
            if let Some(prev) = current_style {
                spans.push(Span::styled(std::mem::take(&mut current), prev));
            }
            current.push(ch);
            current_style = Some(style);
        }
    }
    if let Some(style) = current_style {
        spans.push(Span::styled(current, style));
    }
    spans
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

fn render_working_panel(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect) {
    let (eased, diff_focused) = {
        let view = app.working().unwrap();
        (ease_out_cubic(view.slide), view.focus == Focus::Hunks)
    };
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

    let border = if diff_focused {
        theme.marker
    } else {
        theme.panel_border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(" Uncommitted changes   [Enter] fullscreen ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let scroll = {
        let view = app.working().unwrap();
        let mut lines = working_file_lines(app, view, theme).0;
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "── diff ───────────────",
            Style::default().fg(theme.meta),
        )));
        let focused_hunk = (view.focus == Focus::Hunks).then_some(view.hunk);
        let top = view.diff_scroll as usize;
        let visible = top..top + inner.height as usize;
        match view.diff.as_ref() {
            Some(diff) => {
                lines.extend(diff_lines(diff, focused_hunk, inner.width as usize, visible, theme).0)
            }
            None => lines.extend(no_diff_lines(theme)),
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[space] stage  [Tab] diff  [d] discard  [c] commit",
            Style::default().fg(theme.meta),
        )));

        let max_scroll = lines.len().saturating_sub(inner.height as usize) as u16;
        let scroll = view.diff_scroll.min(max_scroll);
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
        scroll
    };

    if let Some(view) = app.working_mut() {
        view.diff_scroll = scroll;
    }
}

fn render_commit_fullscreen(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect) {
    frame.render_widget(Clear, area);
    let columns = Layout::horizontal([Constraint::Length(46), Constraint::Min(0)]).split(area);

    let (heading, file_lines) = {
        let panel = app.panel().unwrap();
        let heading = app
            .commits()
            .get(panel.commit_index)
            .map(|commit| format!(" {}   {} ", commit.short_id, commit.summary))
            .unwrap_or_else(|| " Commit ".to_string());
        let file_lines: Vec<Line> = panel
            .changed_files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let selected = index == panel.file;
                Line::from(vec![
                    Span::styled(
                        if selected { "❯ " } else { "  " },
                        Style::default().fg(theme.marker),
                    ),
                    status_span(file.status, theme),
                    Span::styled(
                        file.path.clone(),
                        Style::default().fg(if selected { theme.node } else { theme.summary }),
                    ),
                ])
            })
            .collect();
        (heading, file_lines)
    };
    let diff_focused = app.panel().unwrap().diff_focused;
    let files_border = if diff_focused {
        theme.label
    } else {
        theme.marker
    };
    let files_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(files_border))
        .title(heading);
    let file_count = file_lines.len();
    let list_height = columns[0].height.saturating_sub(2) as usize;
    let panel = app.panel_mut().unwrap();
    panel.files_scroll = scroll_into_view(panel.files_scroll, panel.file, list_height, file_count);
    let files_scroll = panel.files_scroll;
    frame.render_widget(
        Paragraph::new(file_lines)
            .block(files_block)
            .scroll((files_scroll, 0)),
        columns[0],
    );

    let panel = app.panel_mut().unwrap();
    render_diff_pane(
        frame,
        panel.diff.as_ref(),
        panel.split,
        None,
        &mut panel.diff_scroll,
        &mut panel.diff_hscroll,
        false,
        diff_focused,
        theme,
        columns[1],
    );
}

fn scroll_into_view(offset: u16, selected: usize, height: usize, total: usize) -> u16 {
    let mut offset = offset as usize;
    if selected < offset {
        offset = selected;
    } else if height > 0 && selected >= offset + height {
        offset = selected + 1 - height;
    }
    offset.min(total.saturating_sub(height)) as u16
}

fn render_working_fullscreen(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect) {
    frame.render_widget(Clear, area);
    let columns = Layout::horizontal([Constraint::Length(46), Constraint::Min(0)]).split(area);

    let diff_focused = app.working().unwrap().focus == Focus::Hunks;
    let files_border = if diff_focused {
        theme.label
    } else {
        theme.marker
    };
    let files_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(files_border))
        .title(" Working directory   [space] stage · [Tab] diff · [c] commit ");
    let (file_lines, selected_row) = {
        let view = app.working().unwrap();
        working_file_lines(app, view, theme)
    };
    let file_count = file_lines.len();
    let list_height = columns[0].height.saturating_sub(2) as usize;
    let view = app.working_mut().unwrap();
    if let Some(row) = selected_row {
        view.files_scroll = scroll_into_view(view.files_scroll, row, list_height, file_count);
    }
    let files_scroll = view.files_scroll;
    frame.render_widget(
        Paragraph::new(file_lines)
            .block(files_block)
            .scroll((files_scroll, 0)),
        columns[0],
    );

    let view = app.working_mut().unwrap();
    let focused_hunk = (view.focus == Focus::Hunks).then_some(view.hunk);
    let snap = view.hunk_snap;
    view.hunk_snap = false;
    render_diff_pane(
        frame,
        view.diff.as_ref(),
        view.split,
        focused_hunk,
        &mut view.diff_scroll,
        &mut view.diff_hscroll,
        snap,
        diff_focused,
        theme,
        columns[1],
    );
}

fn max_content_width(diff: &FileDiff) -> usize {
    diff.hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| content_of(line).chars().count() + 2)
        .max()
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn diff_pane_lines(
    diff: Option<&FileDiff>,
    split: bool,
    focused_hunk: Option<usize>,
    width: usize,
    visible: std::ops::Range<usize>,
    hscroll: usize,
    theme: &Theme,
) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(diff) = diff else {
        return (no_diff_lines(theme), None);
    };
    if split {
        split_diff_lines(diff, focused_hunk, width, hscroll, theme)
    } else {
        diff_lines(diff, focused_hunk, width, visible, theme)
    }
}

fn split_diff_lines(
    diff: &FileDiff,
    focused_hunk: Option<usize>,
    width: usize,
    hscroll: usize,
    theme: &Theme,
) -> (Vec<Line<'static>>, Option<usize>) {
    let syntax = enrich::highlighter().language(&diff.new_path);
    let col_width = width.saturating_sub(SPLIT_SEPARATOR.chars().count()) / 2;
    let mut lines = Vec::new();
    let mut focus_offset = None;
    for (index, hunk) in diff.hunks.iter().enumerate() {
        let focused = focused_hunk == Some(index);
        if focused {
            focus_offset = Some(lines.len());
        }
        lines.push(hunk_header_line(&hunk.header, focused, theme));
        for row in split_rows(&hunk.lines) {
            let (left_emph, right_emph) = match (&row.left, &row.right) {
                (Some(left), Some(right)) if marker_of(left) == '-' && marker_of(right) == '+' => {
                    let spans = word_diff(content_of(left), content_of(right));
                    (emphasis_removed(&spans), emphasis_added(&spans))
                }
                _ => (Vec::new(), Vec::new()),
            };
            let left = row
                .left
                .as_ref()
                .map(|line| diff_body_line(line, &left_emph, syntax, theme));
            let right = row
                .right
                .as_ref()
                .map(|line| diff_body_line(line, &right_emph, syntax, theme));
            lines.push(compose_split_row(left, right, col_width, hscroll, theme));
        }
        lines.push(Line::from(""));
    }
    (lines, focus_offset)
}

fn compose_split_row(
    left: Option<Line<'static>>,
    right: Option<Line<'static>>,
    col_width: usize,
    hscroll: usize,
    theme: &Theme,
) -> Line<'static> {
    let left_spans = left.map(|line| line.spans).unwrap_or_default();
    let (mut spans, left_width) = slice_spans(left_spans, hscroll, col_width);
    if left_width < col_width {
        spans.push(Span::raw(" ".repeat(col_width - left_width)));
    }
    spans.push(Span::styled(
        SPLIT_SEPARATOR.to_string(),
        Style::default().fg(theme.label),
    ));
    if let Some(right) = right {
        let (right_spans, _) = slice_spans(right.spans, hscroll, col_width);
        spans.extend(right_spans);
    }
    Line::from(spans)
}

fn slice_spans(
    spans: Vec<Span<'static>>,
    offset: usize,
    max_width: usize,
) -> (Vec<Span<'static>>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0;
    let mut width = 0;
    for span in spans {
        let chars: Vec<char> = span.content.chars().collect();
        let mut start = 0;
        if skipped < offset {
            let to_skip = (offset - skipped).min(chars.len());
            skipped += to_skip;
            start = to_skip;
        }
        if start >= chars.len() {
            continue;
        }
        let remaining = max_width.saturating_sub(width);
        if remaining == 0 {
            break;
        }
        let take = (chars.len() - start).min(remaining);
        let clipped: String = chars[start..start + take].iter().collect();
        width += take;
        out.push(Span::styled(clipped, span.style));
        if width >= max_width {
            break;
        }
    }
    (out, width)
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

fn render_stash_message(frame: &mut Frame, message: &str, theme: &Theme, area: Rect) {
    let width = 64u16.min(area.width);
    let height = 5u16.min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Stash message   [Enter] stash · [Esc] cancel ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let line = Line::from(vec![
        Span::styled(message.to_string(), Style::default().fg(theme.node)),
        Span::styled("█", Style::default().fg(theme.branch_badge)),
    ]);
    let hint = Line::from(Span::styled(
        "an empty message stashes with git's own",
        Style::default().fg(theme.meta),
    ));
    frame.render_widget(
        Paragraph::new(vec![line, hint]).wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_password_prompt(frame: &mut Frame, prompt: &PasswordPrompt, theme: &Theme, area: Rect) {
    let width = 60u16.min(area.width);
    let height = 5u16.min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warn))
        .title(" Authentication required   [Enter] submit · [Esc] cancel ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let host = Line::from(Span::styled(
        prompt.host().to_string(),
        Style::default().fg(theme.meta),
    ));
    let field = Line::from(vec![
        Span::styled(
            format!("{}: ", prompt.label()),
            Style::default().fg(theme.summary),
        ),
        Span::styled(prompt.shown(), Style::default().fg(theme.node)),
        Span::styled("█", Style::default().fg(theme.branch_badge)),
    ]);
    frame.render_widget(Paragraph::new(vec![host, field]), inner);
}

fn render_focus_panel(frame: &mut Frame, panel: &FocusPanel, theme: &Theme, area: Rect) {
    render_slide_list(
        frame,
        panel,
        theme,
        area,
        SlideList {
            title: " Focus sets ",
            empty: "no saved focus sets",
            hints: &["Enter activate · n save current · d delete"],
            fraction: 0.4,
            min_width: 34.0,
        },
        |set, selected| {
            Line::from(vec![
                Span::styled(
                    if selected { "❯ " } else { "  " },
                    Style::default().fg(theme.marker),
                ),
                Span::styled(
                    set.name.clone(),
                    Style::default().fg(if selected { theme.node } else { theme.summary }),
                ),
                Span::styled(
                    format!(
                        "  {} hidden · {} pinned",
                        set.state.hidden.len(),
                        set.state.pinned.len()
                    ),
                    Style::default().fg(theme.meta),
                ),
            ])
        },
    );
}

fn render_submodule_panel(
    frame: &mut Frame,
    panel: &SubmodulePanel,
    has_parent: bool,
    theme: &Theme,
    area: Rect,
) {
    let Some(rect) = slide_rect(area, panel.slide, 0.45, 56.0) else {
        return;
    };
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Submodules ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    if panel.entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no submodules",
                Style::default().fg(theme.meta),
            ))),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = panel
        .entries
        .iter()
        .enumerate()
        .map(|(index, sub)| submodule_row(sub, index == panel.selected, theme))
        .collect();

    lines.push(Line::default());
    let synced = count_sync(panel, SyncState::Synced);
    let drifted = count_sync(panel, SyncState::Drifted);
    let uninit = count_sync(panel, SyncState::Uninitialized);
    lines.push(Line::from(vec![
        Span::styled(
            format!(" ● {synced} synced"),
            Style::default().fg(theme.added),
        ),
        Span::styled(
            format!("  ◆ {drifted} drifted"),
            Style::default().fg(theme.warn),
        ),
        Span::styled(
            format!("  ○ {uninit} uninit"),
            Style::default().fg(theme.meta),
        ),
    ]));

    let list_height = (lines.len() as u16).min(inner.height);
    frame.render_widget(Paragraph::new(lines), inner);

    if let Some(sub) = panel.focused()
        && inner.height > list_height + 2
    {
        let detail_area = Rect {
            y: inner.y + list_height + 1,
            height: inner.height - list_height - 1,
            ..inner
        };
        frame.render_widget(
            Paragraph::new(submodule_detail(sub, has_parent, theme)),
            detail_area,
        );
    }
}

fn submodule_glyph(sub: &Submodule, theme: &Theme) -> (&'static str, Color) {
    match sub.sync {
        SyncState::Uninitialized => ("○", theme.meta),
        SyncState::Drifted => ("◆", theme.warn),
        SyncState::Synced if sub.is_dirty() => ("●", theme.wip),
        SyncState::Synced => ("●", theme.added),
    }
}

fn short_oid(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

fn submodule_markers(sub: &Submodule, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if sub.sync == SyncState::Drifted
        && let (Some(recorded), Some(checked_out)) = (&sub.recorded, &sub.checked_out)
    {
        spans.push(Span::styled(
            format!(" {}≠{}", short_oid(recorded), short_oid(checked_out)),
            Style::default().fg(theme.warn),
        ));
    }
    if sub.dirty > 0 {
        spans.push(Span::styled(
            format!(" ±{}", sub.dirty),
            Style::default().fg(theme.wip),
        ));
    }
    if sub.untracked > 0 {
        spans.push(Span::styled(
            format!(" ?{}", sub.untracked),
            Style::default().fg(theme.wip),
        ));
    }
    spans
}

fn submodule_row(sub: &Submodule, selected: bool, theme: &Theme) -> Line<'static> {
    let (glyph, glyph_color) = submodule_glyph(sub, theme);
    let mut name_style = Style::default().fg(if selected { theme.node } else { theme.summary });
    if !sub.initialized() {
        name_style = name_style.add_modifier(Modifier::DIM);
    } else if selected {
        name_style = name_style.add_modifier(Modifier::BOLD);
    }
    let mut spans = vec![
        Span::styled(
            if selected { "❯ " } else { "  " },
            Style::default().fg(theme.marker),
        ),
        Span::styled(glyph, Style::default().fg(glyph_color)),
        Span::raw(" "),
        Span::styled(sub.name.clone(), name_style),
    ];
    if sub.initialized() {
        if let Some(branch) = &sub.branch {
            spans.push(Span::styled(
                format!("  {branch}"),
                Style::default().fg(theme.head_badge),
            ));
        } else {
            spans.push(Span::styled("  detached", Style::default().fg(theme.meta)));
        }
        if let Some(checked_out) = &sub.checked_out {
            spans.push(Span::styled(
                format!(" @{}", short_oid(checked_out)),
                Style::default().fg(theme.short_id),
            ));
        }
        spans.extend(submodule_markers(sub, theme));
    } else {
        spans.push(Span::styled(
            "  (uninitialized)",
            Style::default().fg(theme.meta),
        ));
    }
    let mut line = Line::from(spans);
    if selected {
        line = line.style(Style::default().bg(theme.selection_bg));
    }
    line
}

fn submodule_detail(sub: &Submodule, has_parent: bool, theme: &Theme) -> Vec<Line<'static>> {
    let label = Style::default().fg(theme.label);
    let value = Style::default().fg(theme.summary);
    let mut lines = vec![Line::from(Span::styled(
        format!("─ {} ", sub.name),
        Style::default().fg(theme.meta),
    ))];
    if let Some(url) = &sub.url {
        lines.push(Line::from(vec![
            Span::styled(" url       ", label),
            Span::styled(url.clone(), value),
        ]));
    }
    let mut ids = vec![Span::styled(" recorded  ", label)];
    if let Some(recorded) = &sub.recorded {
        ids.push(Span::styled(
            short_oid(recorded).to_string(),
            Style::default().fg(theme.short_id),
        ));
    }
    if let Some(checked_out) = &sub.checked_out {
        ids.push(Span::styled("   checked ", label));
        ids.push(Span::styled(
            short_oid(checked_out).to_string(),
            Style::default().fg(theme.short_id),
        ));
        ids.push(match sub.sync {
            SyncState::Drifted => Span::styled("  ◆ drifted", Style::default().fg(theme.warn)),
            _ => Span::styled("  ● in sync", Style::default().fg(theme.added)),
        });
    }
    lines.push(Line::from(ids));
    if sub.initialized() {
        lines.push(Line::from(vec![
            Span::styled(" worktree  ", label),
            if sub.is_dirty() {
                Span::styled(
                    format!("±{} modified · ?{} untracked", sub.dirty, sub.untracked),
                    Style::default().fg(theme.wip),
                )
            } else {
                Span::styled("clean", Style::default().fg(theme.added))
            },
        ]));
        if let Some(last_commit) = &sub.last_commit {
            lines.push(Line::from(vec![
                Span::styled(" last      ", label),
                Span::styled(last_commit.clone(), value),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            " ○ Not initialized — i clones and checks it out",
            Style::default().fg(theme.wip),
        )));
    }
    lines.push(Line::default());
    let actions = match sub.sync {
        SyncState::Uninitialized => "i init",
        SyncState::Drifted => "Enter enter · u update to recorded",
        SyncState::Synced => "Enter enter",
    };
    let mut hint = vec![Span::styled(
        format!(" {actions}"),
        Style::default().fg(theme.marker),
    )];
    if has_parent {
        hint.push(Span::styled(" · < exit to parent", label));
    }
    lines.push(Line::from(hint));
    lines
}

fn count_sync(panel: &SubmodulePanel, sync: SyncState) -> usize {
    panel.entries.iter().filter(|sub| sub.sync == sync).count()
}

fn render_focus_name(frame: &mut Frame, name: &str, theme: &Theme, area: Rect) {
    let width = 48u16.min(area.width);
    let height = 3u16.min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Save focus as   [Enter] save · [Esc] cancel ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let line = Line::from(vec![
        Span::styled(name.to_string(), Style::default().fg(theme.node)),
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
        .title(if editor.is_from_stash() {
            " Branch from stash   [Enter] create · [Esc] cancel "
        } else {
            " New branch   [Enter] create · [Esc] cancel "
        });
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

fn render_palette(frame: &mut Frame, palette: &Palette, theme: &Theme, area: Rect) {
    let rows = palette.rows();
    let width = 54u16.min(area.width);
    let height = (rows.len() as u16 + 4).clamp(4, area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" Command palette   [Enter] run · [Esc] close ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("› ", Style::default().fg(theme.branch_badge)),
            Span::styled(palette.query.clone(), Style::default().fg(theme.node)),
            Span::styled("█", Style::default().fg(theme.branch_badge)),
        ]),
        Line::from(""),
    ];
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no commands",
            Style::default().fg(theme.meta),
        )));
    }
    for (index, (label, hint)) in rows.iter().enumerate() {
        let selected = index == palette.selected;
        let mut spans = vec![
            Span::styled(
                if selected { "❯ " } else { "  " },
                Style::default().fg(theme.marker),
            ),
            Span::styled(
                (*label).to_string(),
                Style::default().fg(if selected { theme.node } else { theme.summary }),
            ),
        ];
        if !hint.is_empty() {
            spans.push(Span::styled(
                format!("  [{hint}]"),
                Style::default().fg(theme.meta),
            ));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row_text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    fn text_of(rows: &[Vec<Span<'static>>]) -> Vec<String> {
        rows.iter().map(|row| row_text(row)).collect()
    }

    #[test]
    fn short_content_is_a_single_row() {
        let spans = vec![Span::raw("hello world")];
        let rows = wrap_content_rows(&spans, 40);
        assert_eq!(text_of(&rows), vec!["hello world".to_string()]);
    }

    #[test]
    fn long_content_wraps_at_word_boundaries() {
        let spans = vec![Span::raw("the quick brown fox jumps")];
        let rows = wrap_content_rows(&spans, 10);
        assert_eq!(
            text_of(&rows),
            vec![
                "the quick".to_string(),
                "brown fox".to_string(),
                "jumps".to_string(),
            ],
            "wrapping breaks on spaces and drops the breaking space"
        );
    }

    #[test]
    fn a_word_longer_than_the_width_is_hard_broken() {
        let spans = vec![Span::raw("supercalifragilistic")];
        let rows = wrap_content_rows(&spans, 8);
        assert_eq!(
            text_of(&rows),
            vec![
                "supercal".to_string(),
                "ifragili".to_string(),
                "stic".to_string(),
            ]
        );
    }

    #[test]
    fn wrapping_preserves_all_visible_characters_without_the_break_spaces() {
        let spans = vec![Span::raw("alpha beta gamma delta")];
        let rows = wrap_content_rows(&spans, 7);
        let joined: String = text_of(&rows).join(" ");
        assert_eq!(joined, "alpha beta gamma delta");
    }

    #[test]
    fn span_styles_survive_wrapping() {
        let styled = Style::default().fg(Color::Red);
        let spans = vec![Span::styled("aaaa ", styled), Span::raw("bbbb cccc")];
        let rows = wrap_content_rows(&spans, 5);
        assert_eq!(rows[0][0].style, styled, "the first row keeps its color");
    }

    #[test]
    fn wrapped_body_uses_the_marker_gutter_only_on_the_first_row() {
        let gutter = Span::styled("+ ", Style::default().fg(Color::Green));
        let content = vec![Span::raw("one two three four five")];
        let lines = wrap_diff_body(gutter, content, 9);
        assert!(lines.len() > 1, "a long body wraps into several rows");
        assert_eq!(lines[0].spans[0].content.as_ref(), "+ ");
        for line in &lines[1..] {
            assert_eq!(
                line.spans[0].content.as_ref(),
                "  ",
                "continuation rows use a blank hanging-indent gutter"
            );
        }
    }

    #[test]
    fn empty_content_still_yields_one_row() {
        let rows = wrap_content_rows(&[], 10);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_empty());
    }
}
