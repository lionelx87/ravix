use std::path::Path;
use std::time::Duration;

use git2::Oid;

use crate::git::{CommitInfo, FileChange, Repo, RepoMeta};
use crate::graph::{GraphCommit, GraphRow, lay_out};

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
    Reload,
    Tick(Duration),
    Quit,
}

pub struct Panel {
    pub commit_index: usize,
    pub changed_files: Vec<FileChange>,
    pub progress: f32,
    pub target: f32,
}

pub struct App {
    repo: Repo,
    meta: RepoMeta,
    commits: Vec<CommitInfo>,
    rows: Vec<GraphRow>,
    exhausted: bool,
    load_page: usize,
    selected: usize,
    offset: usize,
    page: usize,
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
        let meta = repo.meta()?;
        let load_page = load_page.max(1);
        let commits = repo.commits(0, load_page)?;
        let exhausted = commits.len() < load_page;
        let rows = layout_rows(&commits);
        Ok(Self {
            repo,
            meta,
            commits,
            rows,
            exhausted,
            load_page,
            selected: 0,
            offset: 0,
            page: DEFAULT_PAGE,
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

    pub fn selected_commit(&self) -> Option<&CommitInfo> {
        self.commits.get(self.selected)
    }

    pub fn is_animating(&self) -> bool {
        self.panel
            .as_ref()
            .is_some_and(|panel| panel.progress != panel.target)
    }

    pub fn set_viewport(&mut self, height: usize) {
        self.page = height.max(1);
        self.clamp_offset();
        self.ensure_loaded(self.offset + self.page);
    }

    pub fn update(&mut self, action: Action) {
        match action {
            Action::SelectNext => self.move_selection(1),
            Action::SelectPrev => self.move_selection(-1),
            Action::SelectFirst => self.select(0),
            Action::SelectLast => {
                self.load_all();
                self.select(self.last_index());
            }
            Action::PageDown => self.move_selection(self.page as isize),
            Action::PageUp => self.move_selection(-(self.page as isize)),
            Action::ScrollDown => self.scroll_view(SCROLL_STEP),
            Action::ScrollUp => self.scroll_view(-SCROLL_STEP),
            Action::ClickRow(visible) => self.click_row(visible),
            Action::OpenPanel => self.open_panel(),
            Action::Dismiss => self.dismiss(),
            Action::ToggleHelp => self.help_visible = !self.help_visible,
            Action::Reload => self.reload(),
            Action::Tick(dt) => self.tick(dt),
            Action::Quit => self.should_quit = true,
        }
    }

    fn last_index(&self) -> usize {
        self.commits.len().saturating_sub(1)
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
            self.select(target);
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
        if self.help_visible {
            self.help_visible = false;
        } else if let Some(panel) = &mut self.panel {
            panel.target = 0.0;
        }
    }

    fn reload(&mut self) {
        let selected_id = self.commits.get(self.selected).map(|commit| commit.id);
        let Ok(meta) = self.repo.meta() else {
            return;
        };
        let Ok(commits) = self.repo.commits(0, self.load_page) else {
            return;
        };
        self.meta = meta;
        self.exhausted = commits.len() < self.load_page;
        self.commits = commits;
        self.rebuild_rows();

        self.selected = 0;
        if let Some(id) = selected_id {
            self.selected = self.find_loading(id).unwrap_or(0);
        }
        self.selected = self.selected.min(self.last_index());

        match self.commits.get(self.selected) {
            Some(_) => {
                if let Some(panel) = &mut self.panel {
                    panel.commit_index = self.selected;
                }
            }
            None => self.panel = None,
        }
        self.clamp_offset();
        self.scroll_into_view();
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
        let Some(panel) = &mut self.panel else {
            return;
        };
        let step = dt.as_secs_f32() / PANEL_ANIMATION.as_secs_f32();
        if panel.progress < panel.target {
            panel.progress = (panel.progress + step).min(panel.target);
        } else if panel.progress > panel.target {
            panel.progress = (panel.progress - step).max(panel.target);
        }
        if panel.target == 0.0 && panel.progress <= 0.0 {
            self.panel = None;
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
