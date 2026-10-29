use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::picker::BranchPicker;

use super::{SELECTION_MARKER, Theme, centered};

const BAND_FG: Color = Color::Rgb(20, 24, 32);

pub(super) fn band_tag(label: &str, accent: Color) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(BAND_FG)
            .bg(accent)
            .add_modifier(Modifier::BOLD),
    )
}

pub(super) fn zone_block(
    title: String,
    focused: bool,
    accent: Color,
    theme: &Theme,
) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { accent } else { theme.label }))
        .title(title)
}

pub(super) fn notice_span(text: &str, error: bool, accent: Color, theme: &Theme) -> Span<'static> {
    let color = if error { theme.warn } else { accent };
    Span::styled(format!("{text} "), Style::default().fg(color))
}

pub(super) fn keys_spans(
    keys: &[(&str, &str)],
    accent: Color,
    theme: &Theme,
) -> Vec<Span<'static>> {
    keys.iter()
        .flat_map(|(key, label)| {
            [
                Span::styled(
                    format!(" {key} "),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{label} "), Style::default().fg(theme.meta)),
            ]
        })
        .collect()
}

pub(super) fn render_branch_picker<Context>(
    frame: &mut Frame,
    picker: &BranchPicker<Context>,
    title: String,
    prompt: &str,
    accent: Color,
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
        .border_style(Style::default().fg(accent))
        .title(title);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{prompt} "), Style::default().fg(theme.label)),
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
                Style::default().fg(accent),
            ),
            Span::styled(
                name.to_string(),
                Style::default().fg(if selected { theme.node } else { theme.summary }),
            ),
        ])
    }));
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}
