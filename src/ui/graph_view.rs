use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use std::collections::HashMap;

use git2::Oid;
use unicode_width::UnicodeWidthChar;

use crate::app::App;
use crate::git::{BadgeKind, RefBadge};
use crate::graph::GraphRow;
use crate::visibility::Visibility;

use super::{SELECTION_MARKER, Theme, lane_color, relative_time, sparkle_burst};

const ZEBRA_BG: Color = Color::Rgb(26, 30, 40);
const EXPANDED_REFS_BG: Color = Color::Rgb(36, 44, 58);
const RAIL_RULE: Color = Color::Rgb(62, 68, 84);
const HEAD_ROW_BG: Color = Color::Rgb(13, 74, 80);
const LOCAL_ICON: char = '\u{f0322}';
const REMOTE_ICON: char = '\u{f015f}';
const DETACHED_ICON: char = '\u{f05b}';
const SHA_WIDTH: usize = 8;
const AGE_WIDTH: usize = 5;
const AUTHOR_WIDTH: usize = 14;
const RAIL_MIN: usize = 8;
const RAIL_MAX: usize = 30;
const GRAPH_MAX: usize = 24;
const COLUMN_GAP: usize = 2;
const RAIL_SEPARATOR: &str = " ┊ ";

pub(super) fn render(frame: &mut Frame, app: &App, theme: &Theme, area: Rect, now: i64) {
    if area.height < 2 {
        return;
    }
    let commits = app.commits();
    let rows = app.rows();
    let offset = app.offset();
    let selected = app.selected();
    let badges = &app.meta().badges;
    let drag = app.drag();

    let graph_width = rows
        .iter()
        .map(|row| row.glyphs.chars().count())
        .max()
        .unwrap_or(2)
        .clamp(2, GRAPH_MAX);
    let rail_width = rail_width(app);
    let marker_width = SELECTION_MARKER.chars().count();
    let fixed = rail_width
        + RAIL_SEPARATOR.chars().count()
        + marker_width
        + graph_width
        + COLUMN_GAP
        + COLUMN_GAP
        + AUTHOR_WIDTH
        + COLUMN_GAP
        + AGE_WIDTH
        + COLUMN_GAP
        + SHA_WIDTH;
    let message_width = (area.width as usize).saturating_sub(fixed).max(10);

    let buffer = frame.buffer_mut();
    buffer.set_line(
        area.x,
        area.y,
        &Line::styled(
            header(rail_width, graph_width, message_width),
            Style::default()
                .fg(theme.label)
                .add_modifier(Modifier::BOLD),
        ),
        area.width,
    );

    for row in 0..(area.height as usize - 1) {
        let index = offset + row;
        if index >= commits.len() {
            break;
        }
        let commit = &commits[index];
        let is_selected = index == selected && !app.on_wip();
        let commit_badges = badges.get(&commit.id);
        let is_head = commit_badges.is_some_and(|list| {
            list.iter()
                .any(|badge| matches!(badge.kind, BadgeKind::Head | BadgeKind::CurrentBranch))
        });
        let is_drag_source = drag.is_some_and(|(_, source_row, _)| source_row == index);
        let is_drag_hover = drag.is_some_and(|(_, _, hover)| hover == index);
        let y = area.y + 1 + row as u16;

        let stripe = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };
        let row_style = if is_drag_hover {
            Style::default().bg(theme.add_emph_bg)
        } else if is_drag_source {
            Style::default()
                .bg(theme.selection_bg)
                .add_modifier(Modifier::DIM)
        } else if is_selected {
            Style::default()
                .bg(theme.selection_bg)
                .add_modifier(Modifier::BOLD)
        } else if is_head {
            Style::default().bg(HEAD_ROW_BG)
        } else if index % 2 == 1 {
            Style::default().bg(ZEBRA_BG)
        } else {
            Style::default()
        };
        buffer.set_style(stripe, row_style);

        let celebration = app
            .celebration()
            .filter(|celebration| commit.id.to_string() == celebration.oid);

        let mut spans = Vec::new();
        spans.extend(rail_cell(
            &rail_refs(app, &commit.id),
            rail_width,
            is_head,
            celebration.is_some(),
            theme,
        ));
        spans.push(Span::styled(RAIL_SEPARATOR, Style::default().fg(RAIL_RULE)));
        spans.push(Span::styled(
            if is_selected { SELECTION_MARKER } else { "  " },
            Style::default().fg(theme.marker),
        ));
        spans.extend(graph_cell(&rows[index], badges, theme, graph_width));
        spans.push(Span::raw(" ".repeat(COLUMN_GAP)));
        spans.extend(message_cell(
            &commit.summary,
            message_width,
            celebration.filter(|celebration| celebration.sparkle),
            drag.filter(|&(_, _, hover)| hover == index),
            theme,
        ));
        spans.push(Span::raw(" ".repeat(COLUMN_GAP)));
        spans.push(Span::styled(
            clip_pad(&commit.author_name, AUTHOR_WIDTH),
            Style::default().fg(theme.meta),
        ));
        spans.push(Span::raw(" ".repeat(COLUMN_GAP)));
        spans.push(Span::styled(
            format!("{:>AGE_WIDTH$}", relative_time(now, commit.time)),
            Style::default().fg(theme.meta),
        ));
        spans.push(Span::raw(" ".repeat(COLUMN_GAP)));
        spans.push(Span::styled(
            clip_pad(&commit.short_id, SHA_WIDTH),
            Style::default().fg(theme.short_id),
        ));

        buffer.set_line(area.x, y, &Line::from(spans), area.width);
    }

    if !app.on_wip() {
        render_expanded_refs(buffer, app, theme, area, rail_width);
    }
}

fn render_expanded_refs(
    buffer: &mut ratatui::buffer::Buffer,
    app: &App,
    theme: &Theme,
    area: Rect,
    rail_width: usize,
) {
    let commits = app.commits();
    let offset = app.offset();
    let selected = app.selected();
    let visible_rows = area.height as usize - 1;
    if selected < offset || selected >= offset + visible_rows {
        return;
    }
    let refs = rail_refs(app, &commits[selected].id);
    if refs.len() < 2 {
        return;
    }

    let selected_y = area.y + 1 + (selected - offset) as u16;
    for (position, reference) in refs[1..].iter().enumerate() {
        let y = selected_y + 1 + position as u16;
        if y >= area.y + area.height {
            break;
        }
        let cell = Rect {
            x: area.x,
            y,
            width: rail_width as u16,
            height: 1,
        };
        buffer.set_style(cell, Style::default().bg(EXPANDED_REFS_BG));
        let pill = fitted_pill(reference, rail_width);
        let pad = rail_width.saturating_sub(display_width(&pill));
        let spans = vec![
            Span::raw(" ".repeat(pad)),
            Span::styled(pill, theme.badge_style(reference.kind)),
        ];
        buffer.set_line(area.x, y, &Line::from(spans), rail_width as u16);
    }
}

fn header(rail_width: usize, graph_width: usize, message_width: usize) -> String {
    let mut header = clip_pad(" BRANCH", rail_width);
    header.push_str(&" ".repeat(RAIL_SEPARATOR.chars().count() + 2 + graph_width + COLUMN_GAP));
    header.push_str(&clip_pad("MESSAGE", message_width));
    header.push_str(&" ".repeat(COLUMN_GAP));
    header.push_str(&clip_pad("AUTHOR", AUTHOR_WIDTH));
    header.push_str(&" ".repeat(COLUMN_GAP));
    header.push_str(&format!("{:>AGE_WIDTH$}", "AGE"));
    header.push_str(&" ".repeat(COLUMN_GAP));
    header.push_str(&clip_pad("SHA", SHA_WIDTH));
    header
}

fn rail_refs(app: &App, oid: &Oid) -> Vec<MergedRef> {
    let Some(list) = app.meta().badges.get(oid) else {
        return Vec::new();
    };
    let visibility = app.visibility();
    let head_branch = app.meta().head_branch.as_deref();
    let visible: Vec<RefBadge> = list
        .iter()
        .filter(|badge| badge_is_visible(badge, visibility, head_branch))
        .cloned()
        .collect();
    merged_refs(&visible)
}

fn badge_is_visible(badge: &RefBadge, visibility: &Visibility, head_branch: Option<&str>) -> bool {
    match badge.kind {
        BadgeKind::Head | BadgeKind::CurrentBranch => true,
        BadgeKind::LocalBranch => visibility.is_visible(&badge.label, false),
        BadgeKind::Upstream => {
            let local = badge
                .label
                .split_once('/')
                .map_or(badge.label.as_str(), |(_, rest)| rest);
            visibility.is_visible(local, head_branch == Some(local))
        }
    }
}

fn rail_width(app: &App) -> usize {
    app.commits()
        .iter()
        .filter_map(|commit| {
            let refs = rail_refs(app, &commit.id);
            let first = display_width(&pill_text(refs.first()?));
            let extra = if refs.len() > 1 { 3 } else { 0 };
            Some(first + extra)
        })
        .max()
        .unwrap_or(RAIL_MIN)
        .clamp(RAIL_MIN, RAIL_MAX)
}

fn rail_cell(
    refs: &[MergedRef],
    rail_width: usize,
    is_head: bool,
    celebrating: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let Some(reference) = refs.first() else {
        return vec![Span::raw(" ".repeat(rail_width))];
    };

    let extra = if refs.len() > 1 {
        format!("+{}", refs.len() - 1)
    } else {
        String::new()
    };
    let pill = fitted_pill(reference, rail_width.saturating_sub(display_width(&extra)));
    let pad = rail_width.saturating_sub(display_width(&pill) + display_width(&extra));
    let mut pill_style = if matches!(reference.kind, BadgeKind::Head) {
        Style::default()
            .fg(Color::Rgb(20, 24, 32))
            .bg(theme.wip)
            .add_modifier(Modifier::BOLD)
    } else if is_head {
        Style::default()
            .fg(theme.summary)
            .bg(HEAD_ROW_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        theme.badge_style(reference.kind)
    };
    if is_head && celebrating {
        pill_style = pill_style.add_modifier(Modifier::REVERSED);
    }

    let mut spans = vec![Span::raw(" ".repeat(pad)), Span::styled(pill, pill_style)];
    if !extra.is_empty() {
        spans.push(Span::styled(extra, Style::default().fg(theme.meta)));
    }
    spans
}

fn message_cell(
    summary: &str,
    width: usize,
    sparkle: Option<&crate::app::Celebration>,
    drag_hover: Option<(&str, usize, usize)>,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let burst = sparkle.map(|celebration| sparkle_burst(celebration.progress));
    let hint = drag_hover.map(|(source, _, _)| format!("⟵ drop {source}"));
    let reserved = burst.map_or(0, |text| display_width(text) + 1)
        + hint.as_ref().map_or(0, |text| display_width(text) + 1);

    let text = clip(summary, width.saturating_sub(reserved));
    let mut used = display_width(&text);
    let mut spans = vec![Span::styled(text, Style::default().fg(theme.summary))];
    if let Some(burst) = burst {
        spans.push(Span::styled(
            format!(" {burst}"),
            Style::default().fg(theme.node).add_modifier(Modifier::BOLD),
        ));
        used += display_width(burst) + 1;
    }
    if let Some(hint) = hint {
        used += display_width(&hint) + 1;
        spans.push(Span::styled(
            format!(" {hint}"),
            Style::default()
                .fg(theme.head_badge)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    spans
}

fn graph_cell(
    graph_row: &GraphRow<Oid>,
    badges: &HashMap<Oid, Vec<RefBadge>>,
    theme: &Theme,
    min_width: usize,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut used = 0;
    for (column, glyph) in graph_row.glyphs.chars().enumerate() {
        let key = if glyph == '●' {
            Some(&graph_row.node_key)
        } else {
            graph_row.lane_keys.get(column / 2).and_then(Option::as_ref)
        };
        let color = key
            .map(|key| lane_color(key, badges, theme))
            .unwrap_or(theme.node);
        spans.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
        used += 1;
    }
    if used < min_width {
        spans.push(Span::raw(" ".repeat(min_width - used)));
    }
    spans
}

struct MergedRef {
    label: String,
    kind: BadgeKind,
    local: bool,
    remote: bool,
}

fn merged_refs(list: &[RefBadge]) -> Vec<MergedRef> {
    let mut upstreams: Vec<String> = list
        .iter()
        .filter(|badge| matches!(badge.kind, BadgeKind::Upstream))
        .map(|badge| badge.label.clone())
        .collect();
    let mut merged = Vec::new();
    if let Some(head) = list
        .iter()
        .find(|badge| matches!(badge.kind, BadgeKind::Head))
    {
        merged.push(MergedRef {
            label: head.label.clone(),
            kind: BadgeKind::Head,
            local: false,
            remote: false,
        });
    }
    for badge in list.iter().filter(|badge| {
        matches!(
            badge.kind,
            BadgeKind::CurrentBranch | BadgeKind::LocalBranch
        )
    }) {
        let paired = upstreams
            .iter()
            .position(|upstream| upstream.ends_with(&format!("/{}", badge.label)));
        if let Some(index) = paired {
            upstreams.remove(index);
        }
        merged.push(MergedRef {
            label: badge.label.clone(),
            kind: badge.kind,
            local: true,
            remote: paired.is_some(),
        });
    }
    for label in upstreams {
        merged.push(MergedRef {
            label,
            kind: BadgeKind::Upstream,
            local: false,
            remote: true,
        });
    }
    merged
}

fn pill_text(reference: &MergedRef) -> String {
    pill_text_with(&reference.label, reference)
}

fn pill_text_with(label: &str, reference: &MergedRef) -> String {
    let mut text = String::from(" ");
    if matches!(reference.kind, BadgeKind::CurrentBranch | BadgeKind::Head) {
        text.push_str("✓ ");
    }
    text.push_str(label);
    text.push(' ');
    if matches!(reference.kind, BadgeKind::Head) {
        text.push(DETACHED_ICON);
    }
    if reference.local {
        text.push(LOCAL_ICON);
    }
    if reference.local && reference.remote {
        text.push(' ');
    }
    if reference.remote {
        text.push(REMOTE_ICON);
    }
    text.push(' ');
    text
}

fn fitted_pill(reference: &MergedRef, max_width: usize) -> String {
    let natural = pill_text(reference);
    if display_width(&natural) <= max_width {
        return natural;
    }
    let overhead = display_width(&natural) - display_width(&reference.label);
    let label = clip(&reference.label, max_width.saturating_sub(overhead));
    pill_text_with(&label, reference)
}

fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| character.width().unwrap_or(0))
        .sum()
}

fn clip(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    let mut clipped = String::new();
    let mut used = 0;
    for character in text.chars() {
        let char_width = character.width().unwrap_or(0);
        if used + char_width > width.saturating_sub(1) {
            break;
        }
        clipped.push(character);
        used += char_width;
    }
    clipped.push('…');
    clipped
}

fn clip_pad(text: &str, width: usize) -> String {
    let clipped = clip(text, width);
    let pad = width.saturating_sub(display_width(&clipped));
    format!("{clipped}{}", " ".repeat(pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn badge(label: &str, kind: BadgeKind) -> RefBadge {
        RefBadge {
            label: label.to_string(),
            kind,
        }
    }

    #[test]
    fn a_local_branch_and_its_upstream_merge_into_one_ref() {
        let refs = merged_refs(&[
            badge("main", BadgeKind::CurrentBranch),
            badge("origin/main", BadgeKind::Upstream),
        ]);
        assert_eq!(refs.len(), 1);
        assert!(refs[0].local && refs[0].remote);
        assert_eq!(refs[0].label, "main");
    }

    #[test]
    fn an_unmatched_upstream_stays_as_a_remote_only_ref() {
        let refs = merged_refs(&[
            badge("main", BadgeKind::LocalBranch),
            badge("origin/feature", BadgeKind::Upstream),
        ]);
        assert_eq!(refs.len(), 2);
        assert!(refs[0].local && !refs[0].remote);
        assert!(!refs[1].local && refs[1].remote);
        assert_eq!(refs[1].label, "origin/feature");
    }

    #[test]
    fn the_current_branch_pill_is_checked_and_shows_both_icons() {
        let refs = merged_refs(&[
            badge("main", BadgeKind::CurrentBranch),
            badge("origin/main", BadgeKind::Upstream),
        ]);
        let text = pill_text(&refs[0]);
        assert!(text.contains('✓'));
        assert!(text.contains(LOCAL_ICON));
        assert!(text.contains(REMOTE_ICON));
    }

    #[test]
    fn a_remote_only_ref_shows_just_the_cloud() {
        let refs = merged_refs(&[badge("origin/feature", BadgeKind::Upstream)]);
        let text = pill_text(&refs[0]);
        assert!(!text.contains(LOCAL_ICON));
        assert!(text.contains(REMOTE_ICON));
    }

    #[test]
    fn hidden_branch_badges_are_filtered_along_with_their_upstreams() {
        let mut visibility = Visibility::default();
        visibility.toggle("stale");

        let hidden_local = badge("stale", BadgeKind::LocalBranch);
        let hidden_upstream = badge("origin/stale", BadgeKind::Upstream);
        let current = badge("main", BadgeKind::CurrentBranch);
        let current_upstream = badge("origin/main", BadgeKind::Upstream);

        assert!(!badge_is_visible(&hidden_local, &visibility, Some("main")));
        assert!(!badge_is_visible(
            &hidden_upstream,
            &visibility,
            Some("main")
        ));
        assert!(badge_is_visible(&current, &visibility, Some("main")));
        assert!(badge_is_visible(
            &current_upstream,
            &visibility,
            Some("main")
        ));
    }

    #[test]
    fn a_detached_head_badge_becomes_the_first_pill_with_the_target_icon() {
        let refs = merged_refs(&[
            badge("HEAD", BadgeKind::Head),
            badge("release/32", BadgeKind::LocalBranch),
        ]);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].label, "HEAD");
        let text = pill_text(&refs[0]);
        assert!(text.contains('✓'), "the pill marks where you stand: {text}");
        assert!(
            text.contains(DETACHED_ICON),
            "the pill carries the target icon: {text}"
        );
        assert!(!text.contains(LOCAL_ICON) && !text.contains(REMOTE_ICON));
    }

    #[test]
    fn a_long_branch_name_is_clipped_before_the_icons() {
        let refs = merged_refs(&[
            badge("bugfix/MB2C-5916-google-types", BadgeKind::LocalBranch),
            badge("origin/bugfix/MB2C-5916-google-types", BadgeKind::Upstream),
        ]);
        let pill = fitted_pill(&refs[0], 20);
        assert_eq!(display_width(&pill), 20);
        assert!(pill.contains('…'), "the name carries the ellipsis: {pill}");
        assert!(
            pill.ends_with(&format!("{LOCAL_ICON} {REMOTE_ICON} ")),
            "the icons survive the clipping: {pill}"
        );
    }

    #[test]
    fn a_short_branch_name_keeps_its_natural_pill() {
        let refs = merged_refs(&[badge("main", BadgeKind::LocalBranch)]);
        let pill = fitted_pill(&refs[0], 20);
        assert_eq!(pill, format!(" main {LOCAL_ICON} "));
    }

    #[test]
    fn clip_pad_respects_wide_characters() {
        let padded = clip_pad("日本語", 8);
        assert_eq!(display_width(&padded), 8);
        let clipped = clip_pad("日本語のメッセージ", 8);
        assert_eq!(display_width(&clipped), 8);
        assert!(clipped.contains('…'));
    }
}
