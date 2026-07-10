use std::path::Path;
use std::time::Duration;

use git2::Oid;

use crate::git::{CommitInfo, FileChange, Repo, RepoMeta, StageState, WorkingFile, WorkingStatus};
use crate::graph::{GraphCommit, GraphRow, lay_out};
use crate::mutate::{GitCli, MutationError};
use crate::staging::build_patch;
use crate::undo::{InversePlan, UndoableAction, invert};
use crate::working::{Focus, WorkingView};

const LOAD_PAGE: usize = 500;
const LOAD_MARGIN: usize = 64;
const SCROLL_STEP: isize = 3;
const PANEL_ANIMATION: Duration = Duration::from_millis(160);
const DEFAULT_PAGE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SelectNext,
    SelectPrev,
    SelectFirst,
    SelectLast,
    PageDown,
    PageUp,
    ScrollDown,
    ScrollUp,
    ClickRow(usize),
    OpenPanel,
    Dismiss,
    ToggleHelp,
    ToggleStage,
    StageAll,
    Discard,
    OpenCommit,
    ToggleFocus,
    CommitInput(char),
    CommitBackspace,
    CommitSubmit,
    ConfirmYes,
    ConfirmNo,
    Undo,
    Reload,
    Tick(Duration),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    Graph,
    Working,
    Commit,
    Confirm,
}

pub struct Panel {
    pub commit_index: usize,
    pub changed_files: Vec<FileChange>,
    pub progress: f32,
    pub target: f32,
}

#[derive(Default)]
pub struct CommitEditor {
    pub message: String,
}

enum ConfirmKind {
    Discard { path: String, untracked: bool },
    DiscardHunk { path: String, patch: String },
}

pub struct Confirm {
    pub message: String,
    kind: ConfirmKind,
}

pub struct App {
    repo: Repo,
    cli: GitCli,
    meta: RepoMeta,
    commits: Vec<CommitInfo>,
    rows: Vec<GraphRow>,
    exhausted: bool,
    load_page: usize,
    selected: usize,
    offset: usize,
    page: usize,
    status: WorkingStatus,
    on_wip: bool,
    working: Option<WorkingView>,
    commit: Option<CommitEditor>,
    confirm: Option<Confirm>,
    last_action: Option<UndoableAction>,
    notice: Option<String>,
    panel: Option<Panel>,
    help_visible: bool,
    should_quit: bool,
}

impl App {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, git2::Error> {
        Self::open_with_load_page(path, LOAD_PAGE)
    }

    pub fn open_with_load_page(
        path: impl AsRef<Path>,
        load_page: usize,
    ) -> Result<Self, git2::Error> {
        let repo = Repo::discover(path)?;
        let workdir = repo
            .workdir()
            .unwrap_or_else(|| repo.git_dir())
            .to_path_buf();
        let cli = GitCli::new(workdir);
        let meta = repo.meta()?;
        let load_page = load_page.max(1);
        let commits = repo.commits(0, load_page)?;
        let exhausted = commits.len() < load_page;
        let rows = layout_rows(&commits);
        let status = repo.working_status().unwrap_or_default();
        Ok(Self {
            repo,
            cli,
            meta,
            commits,
            rows,
            exhausted,
            load_page,
            selected: 0,
            offset: 0,
            page: DEFAULT_PAGE,
            status,
            on_wip: false,
            working: None,
            commit: None,
            confirm: None,
            last_action: None,
            notice: None,
            panel: None,
            help_visible: false,
            should_quit: false,
        })
    }

    pub fn git_dir(&self) -> std::path::PathBuf {
        self.repo.git_dir().to_path_buf()
    }

    pub fn meta(&self) -> &RepoMeta {
        &self.meta
    }

    pub fn rows(&self) -> &[GraphRow] {
        &self.rows
    }

    pub fn commits(&self) -> &[CommitInfo] {
        &self.commits
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn panel(&self) -> Option<&Panel> {
        self.panel.as_ref()
    }

    pub fn help_visible(&self) -> bool {
        self.help_visible
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn status(&self) -> &WorkingStatus {
        &self.status
    }

    pub fn has_wip(&self) -> bool {
        !self.status.is_empty()
    }

    pub fn on_wip(&self) -> bool {
        self.on_wip
    }

    pub fn working(&self) -> Option<&WorkingView> {
        self.working.as_ref()
    }

    pub fn commit_editor(&self) -> Option<&CommitEditor> {
        self.commit.as_ref()
    }

    pub fn confirm(&self) -> Option<&Confirm> {
        self.confirm.as_ref()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn working_files(&self) -> Vec<WorkingFile> {
        let mut files = Vec::with_capacity(self.status.total());
        files.extend(self.status.unstaged.iter().cloned());
        files.extend(self.status.staged.iter().cloned());
        files.extend(self.status.untracked.iter().cloned());
        files
    }

    pub fn selected_commit(&self) -> Option<&CommitInfo> {
        self.commits.get(self.selected)
    }

    pub fn input_context(&self) -> InputContext {
        if self.commit.is_some() {
            InputContext::Commit
        } else if self.confirm.is_some() {
            InputContext::Confirm
        } else if self.working.is_some() {
            InputContext::Working
        } else {
            InputContext::Graph
        }
    }

    pub fn is_animating(&self) -> bool {
        self.panel
            .as_ref()
            .is_some_and(|panel| panel.progress != panel.target)
            || self
                .working
                .as_ref()
                .is_some_and(|view| view.slide != view.target)
    }

    pub fn set_viewport(&mut self, height: usize) {
        self.page = height.max(1);
        self.clamp_offset();
        self.ensure_loaded(self.offset + self.page);
    }

    pub fn update(&mut self, action: Action) {
        match action {
            Action::SelectNext => self.move_down(1),
            Action::SelectPrev => self.move_up(1),
            Action::SelectFirst => self.select_first(),
            Action::SelectLast => self.select_last(),
            Action::PageDown => self.move_down(self.page as isize),
            Action::PageUp => self.move_up(self.page as isize),
            Action::ScrollDown => self.scroll_view(SCROLL_STEP),
            Action::ScrollUp => self.scroll_view(-SCROLL_STEP),
            Action::ClickRow(visible) => self.click_row(visible),
            Action::OpenPanel => self.open_or_expand(),
            Action::Dismiss => self.dismiss(),
            Action::ToggleHelp => self.help_visible = !self.help_visible,
            Action::ToggleStage => self.toggle_stage(),
            Action::StageAll => self.stage_all(),
            Action::Discard => self.request_discard(),
            Action::OpenCommit => self.open_commit(),
            Action::ToggleFocus => self.toggle_focus(),
            Action::CommitInput(character) => self.commit_input(character),
            Action::CommitBackspace => self.commit_backspace(),
            Action::CommitSubmit => self.commit_submit(),
            Action::ConfirmYes => self.confirm_yes(),
            Action::ConfirmNo => self.confirm = None,
            Action::Undo => self.perform_undo(),
            Action::Reload => self.reload(),
            Action::Tick(dt) => self.tick(dt),
            Action::Quit => self.should_quit = true,
        }
    }

    fn last_index(&self) -> usize {
        self.commits.len().saturating_sub(1)
    }

    fn move_down(&mut self, delta: isize) {
        if self.working.is_some() {
            self.working_move(delta);
            return;
        }
        if self.on_wip {
            self.on_wip = false;
            self.select(0);
            return;
        }
        self.move_selection(delta);
    }

    fn move_up(&mut self, delta: isize) {
        if self.working.is_some() {
            self.working_move(-delta);
            return;
        }
        if self.on_wip {
            return;
        }
        if self.selected == 0 && self.has_wip() {
            self.on_wip = true;
            return;
        }
        self.move_selection(-delta);
    }

    fn working_move(&mut self, delta: isize) {
        match self.working.as_ref().map(|view| view.focus) {
            Some(Focus::Files) => {
                let len = self.status.total();
                if let Some(view) = &mut self.working {
                    view.move_file(delta, len);
                }
                self.sync_focus_diff();
            }
            Some(Focus::Hunks) => {
                if let Some(view) = &mut self.working {
                    view.move_hunk(delta);
                }
            }
            None => {}
        }
    }

    fn select_first(&mut self) {
        if self.has_wip() {
            self.on_wip = true;
        } else {
            self.select(0);
        }
    }

    fn select_last(&mut self) {
        self.on_wip = false;
        self.load_all();
        self.select(self.last_index());
    }

    fn move_selection(&mut self, delta: isize) {
        let tentative = self.selected as isize + delta;
        if delta > 0 {
            self.ensure_loaded(tentative.max(0) as usize);
        }
        let target = tentative.clamp(0, self.last_index() as isize);
        self.select(target as usize);
    }

    fn select(&mut self, index: usize) {
        if self.commits.is_empty() {
            return;
        }
        self.selected = index.min(self.last_index());
        self.scroll_into_view();
    }

    fn scroll_view(&mut self, delta: isize) {
        let max_offset = self.last_index() as isize;
        self.offset = (self.offset as isize + delta).clamp(0, max_offset) as usize;
        self.ensure_loaded(self.offset + self.page);
        self.clamp_offset();
    }

    fn click_row(&mut self, visible: usize) {
        let target = self.offset + visible;
        if target >= self.commits.len() {
            return;
        }
        if target == self.selected && self.panel.is_none() {
            self.open_panel();
        } else {
            self.on_wip = false;
            self.select(target);
        }
    }

    fn open_or_expand(&mut self) {
        if let Some(view) = &mut self.working {
            view.fullscreen = !view.fullscreen;
        } else if self.on_wip {
            if self.has_wip() {
                self.working = Some(WorkingView::opening());
                self.sync_focus_diff();
            }
        } else {
            self.open_panel();
        }
    }

    fn open_panel(&mut self) {
        let Some(commit) = self.selected_commit() else {
            return;
        };
        let changed_files = self.repo.changed_files(commit.id).unwrap_or_default();
        match &mut self.panel {
            Some(panel) => {
                panel.commit_index = self.selected;
                panel.changed_files = changed_files;
                panel.target = 1.0;
            }
            None => {
                self.panel = Some(Panel {
                    commit_index: self.selected,
                    changed_files,
                    progress: 0.0,
                    target: 1.0,
                });
            }
        }
    }

    fn dismiss(&mut self) {
        if self.commit.is_some() {
            self.commit = None;
        } else if self.confirm.is_some() {
            self.confirm = None;
        } else if self.help_visible {
            self.help_visible = false;
        } else if let Some(view) = &mut self.working {
            if view.focus == Focus::Hunks {
                view.focus = Focus::Files;
            } else if view.fullscreen {
                view.fullscreen = false;
            } else {
                view.target = 0.0;
            }
        } else if let Some(panel) = &mut self.panel {
            panel.target = 0.0;
        }
    }

    fn focused_working_file(&self) -> Option<WorkingFile> {
        let index = self.working.as_ref()?.selected;
        self.working_files().into_iter().nth(index)
    }

    fn toggle_stage(&mut self) {
        let Some(view) = self.working.as_ref() else {
            return;
        };
        match view.focus {
            Focus::Files => {
                let Some(file) = self.focused_working_file() else {
                    return;
                };
                let (result, action) = if file.state == StageState::Staged {
                    (
                        self.cli.unstage_file(&file.path),
                        UndoableAction::Unstaged(file.path.clone()),
                    )
                } else {
                    (
                        self.cli.stage_file(&file.path),
                        UndoableAction::Staged(file.path.clone()),
                    )
                };
                self.after_mutation_recording(result, action);
            }
            Focus::Hunks => {
                let Some((patch, staged_side)) = view
                    .diff
                    .as_ref()
                    .map(|diff| (build_patch(diff, &[view.hunk]), view.staged_side))
                else {
                    return;
                };
                let (result, action) = if staged_side {
                    (
                        self.cli.unstage_hunk(&patch),
                        UndoableAction::UnstagedHunk {
                            patch: patch.clone(),
                        },
                    )
                } else {
                    (
                        self.cli.stage_hunk(&patch),
                        UndoableAction::StagedHunk {
                            patch: patch.clone(),
                        },
                    )
                };
                self.after_mutation_recording(result, action);
            }
        }
    }

    fn stage_all(&mut self) {
        let has_unstaged = !self.status.unstaged.is_empty() || !self.status.untracked.is_empty();
        let (result, action) = if has_unstaged {
            (self.cli.stage_all(), UndoableAction::StagedAll)
        } else {
            (self.cli.unstage_all(), UndoableAction::UnstagedAll)
        };
        self.after_mutation_recording(result, action);
    }

    fn request_discard(&mut self) {
        let Some(view) = self.working.as_ref() else {
            return;
        };
        if view.focus == Focus::Hunks && !view.staged_side {
            let path = self.focused_working_file().map(|file| file.path);
            let patch = view
                .diff
                .as_ref()
                .map(|diff| build_patch(diff, &[view.hunk]));
            if let (Some(path), Some(patch)) = (path, patch) {
                self.confirm = Some(Confirm {
                    message: "Discard this hunk? (y/n)".to_string(),
                    kind: ConfirmKind::DiscardHunk { path, patch },
                });
            }
            return;
        }
        let Some(file) = self.focused_working_file() else {
            return;
        };
        let untracked = file.state == StageState::Untracked;
        self.confirm = Some(Confirm {
            message: format!("Discard changes to {}? (y/n)", file.path),
            kind: ConfirmKind::Discard {
                path: file.path,
                untracked,
            },
        });
    }

    fn confirm_yes(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        match confirm.kind {
            ConfirmKind::Discard { path, untracked } => {
                let snapshot = self.repo.snapshot_blob(&path);
                let result = if untracked {
                    self.cli.remove_untracked(&path)
                } else {
                    self.cli.discard_file(&path)
                };
                self.finish_discard(result, path, snapshot);
            }
            ConfirmKind::DiscardHunk { path, patch } => {
                let snapshot = self.repo.snapshot_blob(&path);
                let result = self.cli.discard_hunk(&patch);
                self.finish_discard(result, path, snapshot);
            }
        }
    }

    fn finish_discard(
        &mut self,
        result: Result<(), MutationError>,
        path: String,
        snapshot: Option<Oid>,
    ) {
        match snapshot {
            Some(oid) => self.after_mutation_recording(
                result,
                UndoableAction::Discarded {
                    path,
                    snapshot: oid.to_string(),
                },
            ),
            None => self.after_mutation(result),
        }
    }

    fn open_commit(&mut self) {
        if self.working.is_none() {
            return;
        }
        if self.status.staged.is_empty() {
            self.notice = Some("Nothing staged to commit".to_string());
            return;
        }
        self.commit = Some(CommitEditor::default());
    }

    fn commit_input(&mut self, character: char) {
        if let Some(editor) = &mut self.commit {
            editor.message.push(character);
        }
    }

    fn commit_backspace(&mut self) {
        if let Some(editor) = &mut self.commit {
            editor.message.pop();
        }
    }

    fn commit_submit(&mut self) {
        let Some(editor) = self.commit.take() else {
            return;
        };
        if editor.message.trim().is_empty() {
            self.notice = Some("Empty commit message".to_string());
            self.commit = Some(editor);
            return;
        }
        let result = self.cli.commit(&editor.message);
        self.after_mutation_recording(result, UndoableAction::Committed);
        self.reload();
    }

    fn toggle_focus(&mut self) {
        if let Some(view) = &mut self.working {
            match view.focus {
                Focus::Files => {
                    if view
                        .diff
                        .as_ref()
                        .is_some_and(|diff| !diff.hunks.is_empty())
                    {
                        view.focus = Focus::Hunks;
                        view.hunk = 0;
                    }
                }
                Focus::Hunks => view.focus = Focus::Files,
            }
        }
    }

    fn sync_focus_diff(&mut self) {
        let Some(file) = self.focused_working_file() else {
            if let Some(view) = &mut self.working {
                view.diff = None;
                view.focus = Focus::Files;
            }
            return;
        };
        let staged = file.state == StageState::Staged;
        let diff = self.repo.file_diff(&file.path, staged).ok().flatten();
        if let Some(view) = &mut self.working {
            view.staged_side = staged;
            match diff.filter(|diff| !diff.hunks.is_empty()) {
                Some(diff) => {
                    view.hunk = view.hunk.min(diff.hunks.len() - 1);
                    view.diff = Some(diff);
                }
                None => {
                    view.diff = None;
                    view.hunk = 0;
                    view.focus = Focus::Files;
                }
            }
        }
    }

    fn after_mutation(&mut self, result: Result<(), MutationError>) {
        self.notice = match result {
            Ok(()) => None,
            Err(error) => Some(error.to_string()),
        };
        self.refresh_status();
    }

    fn after_mutation_recording(
        &mut self,
        result: Result<(), MutationError>,
        action: UndoableAction,
    ) {
        if result.is_ok() {
            self.last_action = Some(action);
        }
        self.after_mutation(result);
    }

    fn perform_undo(&mut self) {
        let Some(action) = self.last_action.take() else {
            self.notice = Some("Nothing to undo".to_string());
            return;
        };
        let (ok, description) = match invert(&action) {
            InversePlan::Stage(path) => (
                self.cli.stage_file(&path).is_ok(),
                format!("Undid unstage of {path}"),
            ),
            InversePlan::Unstage(path) => (
                self.cli.unstage_file(&path).is_ok(),
                format!("Undid stage of {path}"),
            ),
            InversePlan::StageHunk(patch) => (
                self.cli.stage_hunk(&patch).is_ok(),
                "Undid unstage of hunk".to_string(),
            ),
            InversePlan::UnstageHunk(patch) => (
                self.cli.unstage_hunk(&patch).is_ok(),
                "Undid stage of hunk".to_string(),
            ),
            InversePlan::StageAll => (
                self.cli.stage_all().is_ok(),
                "Undid unstage all".to_string(),
            ),
            InversePlan::UnstageAll => (
                self.cli.unstage_all().is_ok(),
                "Undid stage all".to_string(),
            ),
            InversePlan::RestoreFile { path, snapshot } => {
                let restored =
                    Oid::from_str(&snapshot).is_ok_and(|oid| self.repo.restore_blob(&path, oid));
                (restored, format!("Undid discard of {path}"))
            }
            InversePlan::ReflogSoftReset => (
                self.cli.reset_soft_previous().is_ok(),
                "Undid commit".to_string(),
            ),
        };
        if ok {
            self.notice = Some(description);
        } else {
            self.notice = Some("Undo failed".to_string());
            self.last_action = Some(action);
        }
        self.reload();
    }

    fn refresh_status(&mut self) {
        if let Ok(status) = self.repo.working_status() {
            self.status = status;
        }
        if self.status.is_empty() {
            self.on_wip = false;
            self.working = None;
            self.commit = None;
            return;
        }
        let len = self.status.total();
        if let Some(view) = &mut self.working {
            view.reconcile(len);
        }
        if self.working.is_some() {
            self.sync_focus_diff();
        }
    }

    fn reload(&mut self) {
        let selected_id = self.commits.get(self.selected).map(|commit| commit.id);
        if let Ok(meta) = self.repo.meta() {
            self.meta = meta;
        }
        if let Ok(commits) = self.repo.commits(0, self.load_page) {
            self.exhausted = commits.len() < self.load_page;
            self.commits = commits;
            self.rebuild_rows();
        }

        self.selected = selected_id
            .and_then(|id| self.find_loading(id))
            .unwrap_or(0)
            .min(self.last_index());

        if self.commits.get(self.selected).is_none() {
            self.panel = None;
        } else if let Some(panel) = &mut self.panel {
            panel.commit_index = self.selected;
        }
        self.clamp_offset();
        self.scroll_into_view();
        self.refresh_status();
    }

    fn find_loading(&mut self, id: Oid) -> Option<usize> {
        loop {
            if let Some(position) = self.commits.iter().position(|commit| commit.id == id) {
                return Some(position);
            }
            if self.exhausted {
                return None;
            }
            self.load_more();
        }
    }

    fn ensure_loaded(&mut self, index: usize) {
        while !self.exhausted && index + LOAD_MARGIN >= self.commits.len() {
            self.load_more();
        }
    }

    fn load_more(&mut self) {
        match self.repo.commits(self.commits.len(), self.load_page) {
            Ok(mut more) => {
                if more.len() < self.load_page {
                    self.exhausted = true;
                }
                if more.is_empty() {
                    return;
                }
                self.commits.append(&mut more);
                self.rebuild_rows();
            }
            Err(_) => self.exhausted = true,
        }
    }

    fn load_all(&mut self) {
        while !self.exhausted {
            self.load_more();
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows = layout_rows(&self.commits);
    }

    fn tick(&mut self, dt: Duration) {
        let step = dt.as_secs_f32() / PANEL_ANIMATION.as_secs_f32();
        if let Some(panel) = &mut self.panel {
            advance(&mut panel.progress, panel.target, step);
            if panel.target == 0.0 && panel.progress <= 0.0 {
                self.panel = None;
            }
        }
        if let Some(view) = &mut self.working {
            advance(&mut view.slide, view.target, step);
            if view.target == 0.0 && view.slide <= 0.0 {
                self.working = None;
            }
        }
    }

    fn scroll_into_view(&mut self) {
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + self.page {
            self.offset = self.selected + 1 - self.page;
        }
    }

    fn clamp_offset(&mut self) {
        self.offset = self.offset.min(self.last_index());
    }
}

fn advance(value: &mut f32, target: f32, step: f32) {
    if *value < target {
        *value = (*value + step).min(target);
    } else if *value > target {
        *value = (*value - step).max(target);
    }
}

fn layout_rows(commits: &[CommitInfo]) -> Vec<GraphRow> {
    let graph_commits: Vec<GraphCommit<Oid>> = commits
        .iter()
        .map(|commit| GraphCommit {
            id: commit.id,
            parents: commit.parents.clone(),
        })
        .collect();
    lay_out(&graph_commits)
}
