use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::app::{Action, InputContext};

const FILE_PANE_WIDTH: u16 = 46;

#[derive(Default)]
pub struct InputMap {
    pending_g: bool,
}

impl InputMap {
    pub fn on_key(&mut self, key: KeyEvent, context: InputContext) -> Option<Action> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(Action::Quit);
        }
        match context {
            InputContext::Graph => self.on_graph_key(key),
            InputContext::Working => on_working_key(key),
            InputContext::Commit => on_commit_key(key),
            InputContext::Confirm => on_confirm_key(key),
            InputContext::Branch => on_branch_key(key),
            InputContext::BranchName => on_branch_name_key(key),
            InputContext::BranchFilter => on_branch_filter_key(key),
            InputContext::FocusSets => on_focus_key(key),
            InputContext::FocusName => on_focus_name_key(key),
            InputContext::Submodules => on_submodule_key(key),
            InputContext::CommitDiff => on_commit_diff_key(key),
            InputContext::Alert => Some(Action::Dismiss),
            InputContext::Join => on_join_key(key),
            InputContext::Conflict => on_conflict_key(key),
            InputContext::Stash => on_stash_key(key),
            InputContext::Palette => on_palette_key(key),
        }
    }

    fn on_graph_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(Action::OpenPalette);
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

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
            KeyCode::Char('G') => Some(Action::SelectLast),
            KeyCode::PageDown => Some(Action::PageDown),
            KeyCode::PageUp => Some(Action::PageUp),
            KeyCode::Enter => Some(Action::OpenPanel),
            KeyCode::Esc => Some(Action::Dismiss),
            KeyCode::Char(' ') => Some(Action::Checkout),
            KeyCode::Char('M') => Some(Action::OpenJoin),
            KeyCode::Char('f') => Some(Action::Fetch),
            KeyCode::Char('p') => Some(Action::Pull),
            KeyCode::Char('P') => Some(Action::Push),
            KeyCode::Char('s') => Some(Action::StashSave),
            KeyCode::Char('S') => Some(Action::ToggleStashes),
            KeyCode::Char('b') => Some(Action::ToggleBranches),
            KeyCode::Char('F') => Some(Action::ToggleFocusPanel),
            KeyCode::Char('>') => Some(Action::ToggleSubmodulePanel),
            KeyCode::Char('<') => Some(Action::ExitSubmodule),
            KeyCode::Char('n') => Some(Action::NewBranch),
            KeyCode::Char('u') => Some(Action::Undo),
            KeyCode::Char('?') => Some(Action::ToggleHelp),
            KeyCode::Char('q') => Some(Action::Quit),
            _ => None,
        }
    }

    pub fn on_mouse(
        &mut self,
        mouse: MouseEvent,
        graph_area: Rect,
        context: InputContext,
    ) -> Option<Action> {
        let row = || (mouse.row - graph_area.y) as usize;
        let in_graph = contains(graph_area, mouse.column, mouse.row);
        let over_file_list =
            context == InputContext::CommitDiff && mouse.column < graph_area.x + FILE_PANE_WIDTH;
        match mouse.kind {
            MouseEventKind::ScrollDown if over_file_list => Some(Action::ScrollFilesDown),
            MouseEventKind::ScrollUp if over_file_list => Some(Action::ScrollFilesUp),
            MouseEventKind::ScrollDown => Some(Action::ScrollDown),
            MouseEventKind::ScrollUp => Some(Action::ScrollUp),
            MouseEventKind::Down(MouseButton::Left) if in_graph => Some(Action::PointerDown(row())),
            MouseEventKind::Drag(MouseButton::Left) if in_graph => Some(Action::PointerDrag(row())),
            MouseEventKind::Up(MouseButton::Left) if in_graph => Some(Action::PointerUp(row())),
            MouseEventKind::Up(MouseButton::Left) => Some(Action::PointerCancel),
            _ => None,
        }
    }
}

fn on_working_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Char('h') | KeyCode::Left => Some(Action::ScrollDiffLeft),
        KeyCode::Char('l') | KeyCode::Right => Some(Action::ScrollDiffRight),
        KeyCode::Char(' ') => Some(Action::ToggleStage),
        KeyCode::Char('a') => Some(Action::StageAll),
        KeyCode::Char('d') => Some(Action::Discard),
        KeyCode::Char('c') => Some(Action::OpenCommit),
        KeyCode::Char('s') => Some(Action::StashSave),
        KeyCode::Char('b') => Some(Action::ToggleBranches),
        KeyCode::Char('v') => Some(Action::ToggleDiffView),
        KeyCode::Char('u') => Some(Action::Undo),
        KeyCode::Tab | KeyCode::BackTab => Some(Action::ToggleFocus),
        KeyCode::Enter => Some(Action::OpenPanel),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_commit_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => Some(Action::CommitSubmit),
        KeyCode::Backspace => Some(Action::CommitBackspace),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char(character) if !ctrl => Some(Action::CommitInput(character)),
        _ => None,
    }
}

fn on_branch_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Enter => Some(Action::Checkout),
        KeyCode::Char(' ') => Some(Action::ToggleBranchVisibility),
        KeyCode::Char('o') => Some(Action::SoloBranch),
        KeyCode::Char('p') => Some(Action::PinBranch),
        KeyCode::Char('/') => Some(Action::StartBranchFilter),
        KeyCode::Char('M') => Some(Action::OpenJoin),
        KeyCode::Char('n') => Some(Action::NewBranch),
        KeyCode::Char('d') => Some(Action::DeleteBranch),
        KeyCode::Char('b') | KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('u') => Some(Action::Undo),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_focus_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Enter => Some(Action::ActivateFocus),
        KeyCode::Char('n') => Some(Action::SaveFocus),
        KeyCode::Char('d') => Some(Action::DeleteFocus),
        KeyCode::Char('F') | KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_commit_diff_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Char('h') | KeyCode::Left => Some(Action::ScrollDiffLeft),
        KeyCode::Char('l') | KeyCode::Right => Some(Action::ScrollDiffRight),
        KeyCode::Tab | KeyCode::BackTab => Some(Action::ToggleFocus),
        KeyCode::Char('v') => Some(Action::ToggleDiffView),
        KeyCode::Enter | KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_submodule_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Enter => Some(Action::EnterSubmodule),
        KeyCode::Char('>') | KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_focus_name_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => Some(Action::FocusNameSubmit),
        KeyCode::Backspace => Some(Action::FocusNameBackspace),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char(character) if !ctrl => Some(Action::FocusNameInput(character)),
        _ => None,
    }
}

fn on_branch_filter_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => Some(Action::BranchFilterSubmit),
        KeyCode::Backspace => Some(Action::BranchFilterBackspace),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char(character) if !ctrl => Some(Action::BranchFilterInput(character)),
        _ => None,
    }
}

fn on_join_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Enter => Some(Action::ExecuteJoin),
        KeyCode::Char('M') | KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('u') => Some(Action::Undo),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_palette_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => Some(Action::PaletteSubmit),
        KeyCode::Backspace => Some(Action::PaletteBackspace),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('n') if ctrl => Some(Action::SelectNext),
        KeyCode::Char('p') if ctrl => Some(Action::SelectPrev),
        KeyCode::Char(character) if !ctrl => Some(Action::PaletteInput(character)),
        _ => None,
    }
}

fn on_stash_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Char('p') => Some(Action::StashPop),
        KeyCode::Char('a') => Some(Action::StashApply),
        KeyCode::Char('d') => Some(Action::StashDrop),
        KeyCode::Char('S') | KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('u') => Some(Action::Undo),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_conflict_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrev),
        KeyCode::Tab => Some(Action::ConflictNextFile),
        KeyCode::Char('o') => Some(Action::TakeOurs),
        KeyCode::Char('t') => Some(Action::TakeTheirs),
        KeyCode::Char('e') => Some(Action::EditConflict),
        KeyCode::Char('s') => Some(Action::SkipRebase),
        KeyCode::Char('c') => Some(Action::ContinueConflict),
        KeyCode::Char('A') | KeyCode::Esc => Some(Action::AbortConflict),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn on_branch_name_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => Some(Action::BranchNameSubmit),
        KeyCode::Backspace => Some(Action::BranchNameBackspace),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char(character) if !ctrl => Some(Action::BranchNameInput(character)),
        _ => None,
    }
}

fn on_confirm_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => Some(Action::ConfirmYes),
        KeyCode::Char('n') | KeyCode::Esc => Some(Action::ConfirmNo),
        _ => None,
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.x + area.width && row >= area.y && row < area.y + area.height
}
