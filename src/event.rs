use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::app::Action;

#[derive(Default)]
pub struct InputMap {
    pending_g: bool,
}

impl InputMap {
    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind == KeyEventKind::Release {
            return None;
        }

        if key.code == KeyCode::Char('g') && key.modifiers.is_empty() {
            if self.pending_g {
                self.pending_g = false;
                return Some(Action::SelectFirst);
            }
            self.pending_g = true;
            return None;
        }
        self.pending_g = false;

        map_key(key)
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent, graph_area: Rect) -> Option<Action> {
        match mouse.kind {
            MouseEventKind::ScrollDown => Some(Action::ScrollDown),
            MouseEventKind::ScrollUp => Some(Action::ScrollUp),
            MouseEventKind::Down(MouseButton::Left) => {
                if contains(graph_area, mouse.column, mouse.row) {
                    Some(Action::ClickRow((mouse.row - graph_area.y) as usize))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn map_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Char('G') => Some(Action::SelectLast),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Enter => Some(Action::OpenPanel),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.x + area.width && row >= area.y && row < area.y + area.height
}
