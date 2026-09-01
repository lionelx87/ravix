use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use git2::Oid;

use crate::askpass::{
    AskpassConfig, AskpassRequest, AskpassServer, CredentialCache, PromptKind, parse_askpass_prompt,
};
use crate::branches::{BranchEntry, BranchPanel, branch_list};
use crate::celebrate::{Event, Intensity, next_intensity, should_celebrate};
use crate::conflict::{ConflictBrowser, ConflictFile, OpKind, Side, conflict_count, parse};
use crate::drag::{DropIntent, RowRef, resolve_drop};
use crate::focus::{self, FocusSet};
use crate::git::{
    BadgeKind, CONFLICTED, CommitInfo, FileChange, FileStamp, Repo, RepoMeta, RepoStamp,
    StageState, WorkingFile, WorkingStatus,
};
use crate::graph::{GraphCommit, GraphLayout, GraphRow};
use crate::join::{Ancestry, JoinMenu, JoinOption, JoinStrategy, MergePrediction, classify};
use crate::mutate::{GitCli, MutationError};
use crate::palette::fuzzy_filter;
use crate::remote::{
    PullAction, PushState, is_auth_failure, is_non_fast_forward, pull_action, push_state,
};
use crate::slide::{self, SlidePanel, advance};
use crate::staging::{FileDiff, build_patch};
use crate::stash::{
    Focus as StashFocus, StashConflict, StashEntry, StashFile, StashPanel, StashStats, StashView,
};
use crate::submodule::{Submodule, SyncState, breadcrumb_label};
use crate::undo::{InversePlan, UndoableAction, invert};
use crate::visibility::Visibility;
use crate::working::{Focus, WorkingView};

const LOAD_PAGE: usize = 500;
const LOAD_MARGIN: usize = 64;
const SCROLL_STEP: isize = 3;
const PANEL_ANIMATION: Duration = Duration::from_millis(160);
const NAV_TRANSITION: Duration = Duration::from_millis(220);
const CELEBRATION_DURATION: Duration = Duration::from_millis(350);
const PEEK_DEBOUNCE: f32 = 0.08;
const DEFAULT_PAGE: usize = 10;
const STATUS_RETRIES: u8 = 3;

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
    ScrollFilesDown,
    ScrollFilesUp,
    ScrollDiffLeft,
    ScrollDiffRight,
    ClickRow(usize),
    PointerDown(usize),
    PointerDrag(usize),
    PointerUp(usize),
    PointerCancel,
    OpenPanel,
    Dismiss,
    ToggleHelp,
    ToggleStage,
    StageAll,
    Discard,
    OpenCommit,
    ToggleFocus,
    ToggleFocusBack,
    ToggleDiffView,
    CommitInput(char),
    CommitBackspace,
    CommitSubmit,
    ConfirmYes,
    ConfirmNo,
    ToggleBranches,
    Checkout,
    Fetch,
    Pull,
    Push,
    StashSave,
    StashMessageInput(char),
    StashMessageBackspace,
    StashMessageSubmit,
    ToggleStashes,
    StashPop,
    StashApply,
    StashDrop,
    StashRestoreFile,
    StashBranch,
    OpenJoin,
    ExecuteJoin,
    TakeOurs,
    TakeTheirs,
    ContinueConflict,
    AbortConflict,
    EditConflict,
    SkipRebase,
    ConflictNextFile,
    NewBranch,
    DeleteBranch,
    ToggleBranchVisibility,
    SoloBranch,
    PinBranch,
    StartBranchFilter,
    StartStashFilter,
    StashFilterInput(char),
    StashFilterBackspace,
    StashFilterSubmit,
    BranchFilterInput(char),
    BranchFilterBackspace,
    BranchFilterSubmit,
    ToggleFocusPanel,
    SaveFocus,
    FocusNameInput(char),
    FocusNameBackspace,
    FocusNameSubmit,
    ActivateFocus,
    DeleteFocus,
    ToggleSubmodulePanel,
    EnterSubmodule,
    ExitSubmodule,
    InitSubmodule,
    UpdateSubmodule,
    BranchNameInput(char),
    BranchNameBackspace,
    BranchNameSubmit,
    OpenPalette,
    PaletteInput(char),
    PaletteBackspace,
    PaletteSubmit,
    PasswordInput(char),
    PasswordBackspace,
    PasswordSubmit,
    CycleCelebrations,
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
    StashMessage,
    Confirm,
    Branch,
    BranchName,
    BranchFilter,
    StashFilter,
    FocusSets,
    FocusName,
    Submodules,
    CommitDiff,
    Alert,
    Join,
    Conflict,
    Stash,
    Palette,
    Password,
}

pub struct Panel {
    pub commit_index: usize,
    pub changed_files: Vec<FileChange>,
    pub progress: f32,
    pub target: f32,
    pub fullscreen: bool,
    pub file: usize,
    pub diff: Option<FileDiff>,
    pub split: bool,
    pub diff_scroll: u16,
    pub diff_hscroll: u16,
    pub files_scroll: u16,
    pub diff_focused: bool,
}

fn shift_scroll(current: u16, delta: isize) -> u16 {
    (current as isize + delta).max(0) as u16
}

#[derive(Default)]
pub struct CommitEditor {
    pub message: String,
    finish: Option<OpKind>,
}

pub enum BranchStart {
    Commit(String),
    Stash(usize),
}

pub struct BranchCreate {
    pub name: String,
    start: BranchStart,
}

impl BranchCreate {
    pub fn is_from_stash(&self) -> bool {
        matches!(self.start, BranchStart::Stash(_))
    }
}

struct TextFilter {
    query: String,
    editing: bool,
}

pub type FocusPanel = SlidePanel<FocusSet>;
pub type SubmodulePanel = SlidePanel<Submodule>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDirection {
    Push,
    Pop,
}

pub struct NavTransition {
    pub outgoing: Box<App>,
    pub direction: NavDirection,
    pub progress: f32,
}

pub struct DragState {
    down: usize,
    source_branch: Option<String>,
    hover: usize,
}

#[derive(Clone, Copy)]
enum OnComplete {
    Notice(&'static str),
    Push,
    Pull,
}

struct RemoteJob {
    verb: &'static str,
    on_complete: OnComplete,
    spinner: usize,
    rx: mpsc::Receiver<Result<(), MutationError>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DiscardMode {
    Worktree,
    Head,
    Untracked,
}

enum ConfirmKind {
    Discard {
        path: String,
        mode: DiscardMode,
    },
    DiscardHunk {
        path: String,
        patch: String,
    },
    DeleteBranch {
        name: String,
        oid: Option<String>,
        upstream: Option<String>,
    },
    ForceDeleteBranch {
        name: String,
        oid: Option<String>,
        upstream: Option<String>,
    },
    DeleteRemoteBranch {
        remote: String,
        remote_branch: String,
    },
    ForcePush,
    DropStash {
        index: usize,
    },
    AbortStashConflict,
    DeleteFocus {
        name: String,
    },
}

pub struct Confirm {
    pub message: String,
    kind: ConfirmKind,
}

struct Notice {
    text: String,
    error: bool,
}

pub struct PasswordPrompt {
    kind: PromptKind,
    host: String,
    input: String,
    request: AskpassRequest,
}

impl PasswordPrompt {
    pub fn masked(&self) -> bool {
        self.kind == PromptKind::Password
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn label(&self) -> &'static str {
        match self.kind {
            PromptKind::Username => "Username",
            PromptKind::Password => "Password",
        }
    }

    pub fn shown(&self) -> String {
        if self.masked() {
            "•".repeat(self.input.chars().count())
        } else {
            self.input.clone()
        }
    }
}

const COMMANDS: &[(&str, &str, Action)] = &[
    ("Fetch", "f", Action::Fetch),
    ("Pull", "p", Action::Pull),
    ("Push", "P", Action::Push),
    ("Stash changes", "s", Action::StashSave),
    ("Stash list", "S", Action::ToggleStashes),
    ("Branches", "b", Action::ToggleBranches),
    ("Focus sets", "F", Action::ToggleFocusPanel),
    ("Submodules", ">", Action::ToggleSubmodulePanel),
    ("New branch", "n", Action::NewBranch),
    ("Join / merge", "M", Action::OpenJoin),
    ("Undo", "u", Action::Undo),
    ("Cycle celebrations", "", Action::CycleCelebrations),
    ("Help", "?", Action::ToggleHelp),
    ("Quit", "q", Action::Quit),
];

pub struct Celebration {
    pub progress: f32,
    pub oid: String,
    pub sparkle: bool,
}

pub struct Palette {
    pub query: String,
    pub matches: Vec<usize>,
    pub selected: usize,
}

impl Palette {
    fn opening() -> Self {
        Self {
            query: String::new(),
            matches: (0..COMMANDS.len()).collect(),
            selected: 0,
        }
    }

    fn refilter(&mut self) {
        let labels: Vec<&str> = COMMANDS.iter().map(|(label, _, _)| *label).collect();
        self.matches = fuzzy_filter(&self.query, &labels);
        self.selected = 0;
    }

    pub fn rows(&self) -> Vec<(&'static str, &'static str)> {
        self.matches
            .iter()
            .map(|&index| (COMMANDS[index].0, COMMANDS[index].1))
            .collect()
    }

    fn move_selection(&mut self, delta: isize) {
        if self.matches.is_empty() {
            self.selected = 0;
            return;
        }
        let last = (self.matches.len() - 1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }
}

#[derive(Clone, Copy)]
pub struct GraphDims {
    pub rail: usize,
    pub graph: usize,
}

pub struct App {
    repo: Repo,
    cli: GitCli,
    meta: RepoMeta,
    commits: Vec<CommitInfo>,
    rows: Vec<GraphRow<Oid>>,
    graph_dims: Option<GraphDims>,
    peek_debounce: Option<f32>,
    graph_layout: GraphLayout<Oid>,
    commit_oids: Vec<Oid>,
    exhausted: bool,
    load_page: usize,
    selected: usize,
    offset: usize,
    page: usize,
    status: WorkingStatus,
    index_stamp: Option<FileStamp>,
    state_stamp: RepoStamp,
    status_retries: u8,
    on_wip: bool,
    visibility: Visibility,
    working: Option<WorkingView>,
    branch: Option<BranchPanel>,
    branch_filter: Option<TextFilter>,
    branch_all: Vec<BranchEntry>,
    focus_panel: Option<FocusPanel>,
    focus_sets: Vec<FocusSet>,
    focus_name: Option<String>,
    submodule_panel: Option<SubmodulePanel>,
    nav_transition: Option<NavTransition>,
    parent_sync: Option<SyncState>,
    breadcrumb: Vec<PathBuf>,
    join: Option<JoinMenu>,
    conflict: Option<ConflictBrowser>,
    edit_request: Option<String>,
    drag: Option<DragState>,
    stash: Option<StashView>,
    stash_message: Option<String>,
    stash_all: Vec<StashEntry>,
    stash_filter: Option<TextFilter>,
    stash_conflict: Option<StashConflict>,
    remote: Option<RemoteJob>,
    askpass_sock: Option<PathBuf>,
    askpass_requests: Option<mpsc::Receiver<AskpassRequest>>,
    askpass_cancelled: bool,
    credentials: CredentialCache,
    askpass_used: Vec<String>,
    password: Option<PasswordPrompt>,
    branch_create: Option<BranchCreate>,
    commit: Option<CommitEditor>,
    confirm: Option<Confirm>,
    alert: Option<String>,
    celebration: Option<Celebration>,
    intensity: Intensity,
    last_action: Option<UndoableAction>,
    notice: Option<Notice>,
    panel: Option<Panel>,
    palette: Option<Palette>,
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
        let visibility = Visibility::default();
        let commit_oids = repo.commit_oids(&visibility)?;
        let take = load_page.min(commit_oids.len());
        let commits = repo.hydrate(&commit_oids[..take]);
        let exhausted = commits.len() >= commit_oids.len();
        let mut graph_layout = GraphLayout::new();
        let rows = graph_layout.extend(&graph_commits(&commits));
        let index_stamp = repo.index_stamp();
        let state_stamp = repo.state_stamp();
        let status = repo.working_status().unwrap_or_default();
        let mut app = Self {
            repo,
            cli,
            meta,
            commits,
            rows,
            graph_dims: None,
            peek_debounce: None,
            graph_layout,
            commit_oids,
            exhausted,
            load_page,
            selected: 0,
            offset: 0,
            page: DEFAULT_PAGE,
            status,
            index_stamp,
            state_stamp,
            status_retries: 0,
            on_wip: false,
            visibility,
            working: None,
            branch: None,
            branch_filter: None,
            branch_all: Vec::new(),
            focus_panel: None,
            focus_sets: Vec::new(),
            focus_name: None,
            submodule_panel: None,
            nav_transition: None,
            parent_sync: None,
            breadcrumb: Vec::new(),
            join: None,
            conflict: None,
            edit_request: None,
            drag: None,
            stash: None,
            stash_message: None,
            stash_all: Vec::new(),
            stash_filter: None,
            stash_conflict: None,
            remote: None,
            askpass_sock: None,
            askpass_requests: None,
            askpass_cancelled: false,
            credentials: CredentialCache::default(),
            askpass_used: Vec::new(),
            password: None,
            branch_create: None,
            commit: None,
            confirm: None,
            alert: None,
            celebration: None,
            intensity: Intensity::Full,
            last_action: None,
            notice: None,
            panel: None,
            palette: None,
            help_visible: false,
            should_quit: false,
        };
        app.detect_conflict();
        app.focus_sets = app.load_focus_sets();
        Ok(app)
    }

    pub fn git_dir(&self) -> std::path::PathBuf {
        self.repo.git_dir().to_path_buf()
    }

    pub fn workdir(&self) -> Option<std::path::PathBuf> {
        self.repo.workdir().map(Path::to_path_buf)
    }

    fn focus_path(&self) -> std::path::PathBuf {
        self.repo.git_dir().join("ravix").join("focus")
    }

    fn load_focus_sets(&self) -> Vec<FocusSet> {
        std::fs::read_to_string(self.focus_path())
            .map(|text| focus::parse(&text))
            .unwrap_or_default()
    }

    fn write_focus_sets(&self) {
        let path = self.focus_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, focus::serialize(&self.focus_sets));
    }

    pub fn meta(&self) -> &RepoMeta {
        &self.meta
    }

    pub fn rows(&self) -> &[GraphRow<Oid>] {
        &self.rows
    }

    pub fn graph_dims(&self) -> Option<GraphDims> {
        self.graph_dims
    }

    pub fn set_graph_dims(&mut self, dims: GraphDims) {
        self.graph_dims = Some(dims);
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

    pub fn panel_mut(&mut self) -> Option<&mut Panel> {
        self.panel.as_mut()
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

    pub fn working_mut(&mut self) -> Option<&mut WorkingView> {
        self.working.as_mut()
    }

    pub fn branch_panel(&self) -> Option<&BranchPanel> {
        self.branch.as_ref()
    }

    pub fn visibility(&self) -> &Visibility {
        &self.visibility
    }

    pub fn branch_filter_query(&self) -> Option<&str> {
        self.branch_filter
            .as_ref()
            .map(|filter| filter.query.as_str())
    }

    pub fn branch_filter_editing(&self) -> bool {
        self.branch_filter
            .as_ref()
            .is_some_and(|filter| filter.editing)
    }

    pub fn focus_panel(&self) -> Option<&FocusPanel> {
        self.focus_panel.as_ref()
    }

    pub fn focus_name(&self) -> Option<&str> {
        self.focus_name.as_deref()
    }

    pub fn submodule_panel(&self) -> Option<&SubmodulePanel> {
        self.submodule_panel.as_ref()
    }

    pub fn nav_transition(&self) -> Option<&NavTransition> {
        self.nav_transition.as_ref()
    }

    pub fn take_nav_transition(&mut self) -> Option<NavTransition> {
        self.nav_transition.take()
    }

    pub fn restore_nav_transition(&mut self, transition: NavTransition) {
        self.nav_transition = Some(transition);
    }

    pub fn head_short_id(&self) -> Option<String> {
        self.repo.head_short_id()
    }

    pub fn parent_sync(&self) -> Option<SyncState> {
        self.parent_sync
    }

    pub fn breadcrumb(&self) -> Option<String> {
        if self.breadcrumb.is_empty() {
            return None;
        }
        Some(breadcrumb_label(&self.breadcrumb, &self.current_workdir()))
    }

    pub fn join_menu(&self) -> Option<&JoinMenu> {
        self.join.as_ref()
    }

    pub fn conflict_browser(&self) -> Option<&ConflictBrowser> {
        self.conflict.as_ref()
    }

    pub fn stash_message(&self) -> Option<&str> {
        self.stash_message.as_deref()
    }

    pub fn stash_panel(&self) -> Option<&StashPanel> {
        self.stash.as_ref().map(|view| &view.panel)
    }

    pub fn stash_view(&self) -> Option<&StashView> {
        self.stash.as_ref()
    }

    pub fn stash_view_mut(&mut self) -> Option<&mut StashView> {
        self.stash.as_mut()
    }

    pub fn stash_stats(&self, entry: &crate::stash::StashEntry) -> StashStats {
        let Ok(oid) = Oid::from_str(&entry.oid) else {
            return StashStats::default();
        };
        let mut stats = StashStats::default();
        for id in std::iter::once(oid).chain(self.repo.untracked_parent(oid)) {
            stats.files += self
                .repo
                .changed_files(id)
                .map(|files| files.len())
                .unwrap_or(0);
            for lines in self.repo.change_stats(id).values() {
                stats.lines.added += lines.added;
                stats.lines.removed += lines.removed;
            }
        }
        stats
    }

    pub fn palette(&self) -> Option<&Palette> {
        self.palette.as_ref()
    }

    pub fn password_prompt(&self) -> Option<&PasswordPrompt> {
        self.password.as_ref()
    }

    pub fn drag(&self) -> Option<(&str, usize, usize)> {
        let drag = self.drag.as_ref()?;
        let source = drag.source_branch.as_deref()?;
        (drag.hover != drag.down).then_some((source, drag.down, drag.hover))
    }

    pub fn branch_create(&self) -> Option<&BranchCreate> {
        self.branch_create.as_ref()
    }

    pub fn commit_editor(&self) -> Option<&CommitEditor> {
        self.commit.as_ref()
    }

    pub fn confirm(&self) -> Option<&Confirm> {
        self.confirm.as_ref()
    }

    pub fn alert(&self) -> Option<&str> {
        self.alert.as_deref()
    }

    pub fn celebration(&self) -> Option<&Celebration> {
        self.celebration.as_ref()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_ref().map(|notice| notice.text.as_str())
    }

    pub fn notice_is_error(&self) -> bool {
        self.notice.as_ref().is_some_and(|notice| notice.error)
    }

    fn info(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: false,
        });
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: true,
        });
    }

    pub fn remote_status(&self) -> Option<(&'static str, usize)> {
        self.remote.as_ref().map(|job| (job.verb, job.spinner))
    }

    pub fn remote_pending(&self) -> bool {
        self.remote.is_some()
    }

    pub fn head_tracking(&self) -> Option<(usize, usize)> {
        self.repo.head_tracking()
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

    pub fn head_commit(&self) -> Option<&CommitInfo> {
        let head = self.repo.head_oid()?;
        self.commits
            .iter()
            .find(|commit| commit.id.to_string() == head)
    }

    pub fn input_context(&self) -> InputContext {
        if self.password.is_some() {
            InputContext::Password
        } else if self.alert.is_some() {
            InputContext::Alert
        } else if self.palette.is_some() {
            InputContext::Palette
        } else if self.stash_message.is_some() {
            InputContext::StashMessage
        } else if self.commit.is_some() {
            InputContext::Commit
        } else if self.branch_create.is_some() {
            InputContext::BranchName
        } else if self.confirm.is_some() {
            InputContext::Confirm
        } else if self.conflict.is_some() {
            InputContext::Conflict
        } else if self
            .stash_filter
            .as_ref()
            .is_some_and(|filter| filter.editing)
        {
            InputContext::StashFilter
        } else if self.stash.is_some() {
            InputContext::Stash
        } else if self.join.is_some() {
            InputContext::Join
        } else if self.focus_name.is_some() {
            InputContext::FocusName
        } else if self
            .branch_filter
            .as_ref()
            .is_some_and(|filter| filter.editing)
        {
            InputContext::BranchFilter
        } else if self.focus_panel.is_some() {
            InputContext::FocusSets
        } else if self.submodule_panel.is_some() {
            InputContext::Submodules
        } else if self.branch.is_some() {
            InputContext::Branch
        } else if self.working.is_some() {
            InputContext::Working
        } else if self.panel.as_ref().is_some_and(|panel| panel.fullscreen) {
            InputContext::CommitDiff
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
            || slide::is_animating(&self.branch)
            || self
                .join
                .as_ref()
                .is_some_and(|menu| menu.panel.is_sliding())
            || self
                .stash
                .as_ref()
                .is_some_and(|view| view.panel.is_sliding())
            || slide::is_animating(&self.focus_panel)
            || slide::is_animating(&self.submodule_panel)
            || self.celebration.is_some()
            || self.remote.is_some()
            || self.nav_transition.is_some()
            || self.peek_debounce.is_some()
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
            Action::PageDown => {
                if !self.scroll_diff(self.page as isize) {
                    self.move_down(self.page as isize);
                }
            }
            Action::PageUp => {
                if !self.scroll_diff(-(self.page as isize)) {
                    self.move_up(self.page as isize);
                }
            }
            Action::ScrollDown => {
                if !self.scroll_diff(SCROLL_STEP) {
                    self.scroll_view(SCROLL_STEP);
                }
            }
            Action::ScrollUp => {
                if !self.scroll_diff(-SCROLL_STEP) {
                    self.scroll_view(-SCROLL_STEP);
                }
            }
            Action::ScrollFilesDown => self.scroll_files(1),
            Action::ScrollFilesUp => self.scroll_files(-1),
            Action::ScrollDiffLeft => self.scroll_diff_horizontal(-SCROLL_STEP),
            Action::ScrollDiffRight => self.scroll_diff_horizontal(SCROLL_STEP),
            Action::ClickRow(visible) => self.click_row(visible),
            Action::PointerDown(visible) => self.pointer_down(visible),
            Action::PointerDrag(visible) => self.pointer_drag(visible),
            Action::PointerUp(visible) => self.pointer_up(visible),
            Action::PointerCancel => self.drag = None,
            Action::OpenPanel => self.open_or_expand(),
            Action::Dismiss => self.dismiss(),
            Action::ToggleHelp => self.help_visible = !self.help_visible,
            Action::ToggleStage => self.toggle_stage(),
            Action::StageAll => self.stage_all(),
            Action::Discard => self.request_discard(),
            Action::OpenCommit => self.open_commit(),
            Action::ToggleFocus => self.toggle_focus(),
            Action::ToggleFocusBack => self.toggle_focus_back(),
            Action::ToggleDiffView => self.toggle_diff_view(),
            Action::CommitInput(character) => self.commit_input(character),
            Action::CommitBackspace => self.commit_backspace(),
            Action::CommitSubmit => self.commit_submit(),
            Action::PasswordInput(character) => self.password_input(character),
            Action::PasswordBackspace => self.password_backspace(),
            Action::PasswordSubmit => self.password_submit(),
            Action::ConfirmYes => self.confirm_yes(),
            Action::ConfirmNo => self.confirm = None,
            Action::ToggleBranches => self.toggle_branches(),
            Action::Checkout => self.checkout(),
            Action::Fetch => self.fetch(),
            Action::Pull => self.pull(),
            Action::Push => self.push(),
            Action::StashSave => self.ask_stash_message(),
            Action::StashMessageInput(character) => {
                if let Some(message) = &mut self.stash_message {
                    message.push(character);
                }
            }
            Action::StashMessageBackspace => {
                if let Some(message) = &mut self.stash_message {
                    message.pop();
                }
            }
            Action::StashMessageSubmit => self.stash_save(),
            Action::ToggleStashes => self.toggle_stashes(),
            Action::StashPop => self.stash_pop(),
            Action::StashApply => self.stash_apply(),
            Action::StashDrop => self.request_drop_stash(),
            Action::StashRestoreFile => self.stash_restore_file(),
            Action::StashBranch => self.ask_stash_branch(),
            Action::OpenPalette => self.palette = Some(Palette::opening()),
            Action::PaletteInput(character) => self.palette_input(character),
            Action::PaletteBackspace => self.palette_backspace(),
            Action::PaletteSubmit => self.palette_submit(),
            Action::OpenJoin => self.open_join(),
            Action::ExecuteJoin => self.execute_join(),
            Action::TakeOurs => self.resolve_block(Side::Ours),
            Action::TakeTheirs => self.resolve_block(Side::Theirs),
            Action::ContinueConflict => self.continue_conflict(),
            Action::AbortConflict => self.abort_conflict(),
            Action::EditConflict => self.request_edit(),
            Action::SkipRebase => self.skip_rebase(),
            Action::ConflictNextFile => {
                if let Some(browser) = &mut self.conflict {
                    browser.move_file(1);
                }
            }
            Action::NewBranch => self.open_branch_create(),
            Action::DeleteBranch => self.request_delete_branch(),
            Action::ToggleBranchVisibility => self.toggle_branch_visibility(),
            Action::SoloBranch => self.solo_branch(),
            Action::PinBranch => self.pin_branch(),
            Action::StartBranchFilter => self.start_branch_filter(),
            Action::StartStashFilter => self.start_stash_filter(),
            Action::StashFilterInput(character) => self.stash_filter_input(character),
            Action::StashFilterBackspace => self.stash_filter_backspace(),
            Action::StashFilterSubmit => self.stash_filter_submit(),
            Action::BranchFilterInput(character) => self.branch_filter_input(character),
            Action::BranchFilterBackspace => self.branch_filter_backspace(),
            Action::BranchFilterSubmit => self.branch_filter_submit(),
            Action::ToggleFocusPanel => self.toggle_focus_panel(),
            Action::SaveFocus => self.start_save_focus(),
            Action::FocusNameInput(character) => self.focus_name_input(character),
            Action::FocusNameBackspace => self.focus_name_backspace(),
            Action::FocusNameSubmit => self.focus_name_submit(),
            Action::ActivateFocus => self.activate_focus(),
            Action::DeleteFocus => self.request_delete_focus(),
            Action::ToggleSubmodulePanel => self.toggle_submodule_panel(),
            Action::EnterSubmodule => self.enter_submodule(),
            Action::ExitSubmodule => self.exit_submodule(),
            Action::InitSubmodule => self.init_submodule(),
            Action::UpdateSubmodule => self.update_submodule(),
            Action::BranchNameInput(character) => self.branch_name_input(character),
            Action::BranchNameBackspace => self.branch_name_backspace(),
            Action::BranchNameSubmit => self.branch_name_submit(),
            Action::CycleCelebrations => self.cycle_celebrations(),
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
        if let Some(panel) = self.panel.as_ref().filter(|panel| panel.fullscreen) {
            if panel.diff_focused {
                self.scroll_diff(delta);
            } else {
                self.commit_diff_move(delta);
            }
            return;
        }
        if let Some(palette) = &mut self.palette {
            palette.move_selection(delta);
            return;
        }
        if let Some(browser) = &mut self.conflict {
            browser.move_block(delta);
            return;
        }
        if let Some(panel) = &mut self.focus_panel {
            panel.move_selection(delta);
            return;
        }
        if let Some(panel) = &mut self.submodule_panel {
            panel.move_selection(delta);
            return;
        }
        if self.stash.is_some() {
            self.stash_move(delta);
            return;
        }
        if let Some(menu) = &mut self.join {
            menu.panel.move_selection(delta);
            return;
        }
        if let Some(panel) = &mut self.branch {
            panel.move_selection(delta);
            return;
        }
        if let Some(view) = self.working.as_ref() {
            if view.fullscreen {
                self.working_move(delta);
                return;
            }
            if view.focus == Focus::Hunks {
                self.scroll_diff(delta);
                return;
            }
        }
        if self.on_wip {
            self.on_wip = false;
            self.select(0);
            return;
        }
        self.move_selection(delta);
    }

    fn move_up(&mut self, delta: isize) {
        if let Some(panel) = self.panel.as_ref().filter(|panel| panel.fullscreen) {
            if panel.diff_focused {
                self.scroll_diff(-delta);
            } else {
                self.commit_diff_move(-delta);
            }
            return;
        }
        if let Some(palette) = &mut self.palette {
            palette.move_selection(-delta);
            return;
        }
        if let Some(browser) = &mut self.conflict {
            browser.move_block(-delta);
            return;
        }
        if let Some(panel) = &mut self.focus_panel {
            panel.move_selection(-delta);
            return;
        }
        if let Some(panel) = &mut self.submodule_panel {
            panel.move_selection(-delta);
            return;
        }
        if self.stash.is_some() {
            self.stash_move(-delta);
            return;
        }
        if let Some(menu) = &mut self.join {
            menu.panel.move_selection(-delta);
            return;
        }
        if let Some(panel) = &mut self.branch {
            panel.move_selection(-delta);
            return;
        }
        if let Some(view) = self.working.as_ref() {
            if view.fullscreen {
                self.working_move(-delta);
                return;
            }
            if view.focus == Focus::Hunks {
                self.scroll_diff(-delta);
                return;
            }
        }
        if self.on_wip {
            return;
        }
        if self.selected == 0 && self.has_wip() {
            self.on_wip = true;
            self.sync_peek();
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
        self.sync_peek();
    }

    fn peek_active(&self) -> bool {
        self.panel.as_ref().is_some_and(|panel| !panel.fullscreen)
            || self.working.as_ref().is_some_and(|view| !view.fullscreen)
    }

    fn sync_peek(&mut self) {
        if !self.peek_active() {
            return;
        }
        if self.on_wip {
            self.panel = None;
            if self.working.is_none() && self.has_wip() {
                self.working = Some(WorkingView::opening());
                self.sync_focus_diff();
            }
        } else {
            self.working = None;
            self.open_panel();
        }
    }

    fn scroll_diff(&mut self, delta: isize) -> bool {
        if let Some(view) = self
            .stash
            .as_mut()
            .filter(|view| view.fullscreen || view.focus == StashFocus::Hunks)
        {
            view.diff_scroll = shift_scroll(view.diff_scroll, delta);
            return true;
        }
        if let Some(panel) = self.panel.as_mut().filter(|panel| panel.fullscreen) {
            panel.diff_scroll = shift_scroll(panel.diff_scroll, delta);
            return true;
        }
        if let Some(view) = self
            .working
            .as_mut()
            .filter(|view| view.fullscreen || view.focus == Focus::Hunks)
        {
            view.diff_scroll = shift_scroll(view.diff_scroll, delta);
            return true;
        }
        false
    }

    fn scroll_files(&mut self, delta: isize) {
        if self.panel.as_ref().is_some_and(|panel| panel.fullscreen) {
            self.commit_diff_move(delta);
        } else if self
            .working
            .as_ref()
            .is_some_and(|view| view.fullscreen && view.focus == Focus::Files)
        {
            self.working_move(delta);
        }
    }

    fn scroll_diff_horizontal(&mut self, delta: isize) {
        if let Some(view) = self
            .stash
            .as_mut()
            .filter(|view| view.fullscreen && view.split)
        {
            view.diff_hscroll = shift_scroll(view.diff_hscroll, delta);
            return;
        }
        if let Some(panel) = self
            .panel
            .as_mut()
            .filter(|panel| panel.fullscreen && panel.split)
        {
            panel.diff_hscroll = shift_scroll(panel.diff_hscroll, delta);
        } else if let Some(view) = self
            .working
            .as_mut()
            .filter(|view| view.fullscreen && view.split)
        {
            view.diff_hscroll = shift_scroll(view.diff_hscroll, delta);
        }
    }

    fn scroll_view(&mut self, delta: isize) {
        let max_offset = self.last_index() as isize;
        self.offset = (self.offset as isize + delta).clamp(0, max_offset) as usize;
        self.ensure_loaded(self.offset + self.page);
        self.clamp_offset();
    }

    fn row_to_index(&self, visible: usize) -> Option<usize> {
        let header_rows = self.has_wip() as usize + 1;
        if visible < header_rows {
            return None;
        }
        let index = self.offset + (visible - header_rows);
        (index < self.commits.len()).then_some(index)
    }

    fn click_row(&mut self, visible: usize) {
        let Some(target) = self.row_to_index(visible) else {
            return;
        };
        if target == self.selected && self.panel.is_none() {
            self.open_panel();
        } else {
            self.on_wip = false;
            self.select(target);
        }
    }

    fn pointer_down(&mut self, visible: usize) {
        if self.input_context() != InputContext::Graph {
            return;
        }
        let Some(index) = self.row_to_index(visible) else {
            self.drag = None;
            return;
        };
        let source_branch = self.local_branch_at(self.commits[index].id);
        self.drag = Some(DragState {
            down: index,
            source_branch,
            hover: index,
        });
    }

    fn pointer_drag(&mut self, visible: usize) {
        let Some(index) = self.row_to_index(visible) else {
            return;
        };
        if let Some(drag) = &mut self.drag {
            drag.hover = index;
        }
    }

    fn pointer_up(&mut self, visible: usize) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some(up) = self.row_to_index(visible) else {
            return;
        };
        let rows: Vec<RowRef> = self
            .commits
            .iter()
            .map(|commit| RowRef {
                oid: commit.id.to_string(),
                branch: self.local_branch_at(commit.id),
            })
            .collect();
        match resolve_drop(&rows, drag.down, up) {
            Some(intent) => self.perform_drop(intent),
            None => self.click_row(visible),
        }
    }

    fn perform_drop(&mut self, intent: DropIntent) {
        let on_target = self.repo.head_oid().as_deref() == Some(intent.target_oid.as_str());
        if !on_target {
            match &intent.target_branch {
                Some(branch) => self.perform_checkout(branch.clone(), false),
                None => self.perform_checkout(intent.target_oid.clone(), true),
            }
            if self.repo.head_oid().as_deref() != Some(intent.target_oid.as_str()) {
                return;
            }
        }
        self.open_join_with_source(intent.source_branch);
    }

    fn open_join_with_source(&mut self, name: String) {
        let Some(oid) = self.repo.branch_tip(&name) else {
            return;
        };
        self.open_join_for(Some(name), oid);
    }

    fn open_join_for(&mut self, name: Option<String>, oid: String) {
        let head = self.repo.head_oid().unwrap_or_default();
        if oid == head {
            self.info("Source is already at HEAD".to_string());
            return;
        }
        self.join = Some(self.build_join_menu(name, oid, &head));
    }

    fn remote_busy(&mut self) -> bool {
        if self.remote.is_some() {
            self.info("A remote operation is already running".to_string());
            return true;
        }
        false
    }

    fn fetch(&mut self) {
        if self.remote_busy() {
            return;
        }
        self.spawn_remote("fetching", OnComplete::Notice("Fetched"), |cli| cli.fetch());
    }

    fn pull(&mut self) {
        if self.remote_busy() {
            return;
        }
        if self.repo.head_upstream().is_none() {
            self.info("No upstream to pull".to_string());
            return;
        }
        self.spawn_remote("pulling", OnComplete::Pull, |cli| cli.fetch());
    }

    fn pull_integrate(&mut self) {
        let Some((up_name, up_oid)) = self.repo.head_upstream() else {
            self.info("No upstream to pull".to_string());
            return;
        };
        let head = self.repo.head_oid().unwrap_or_default();
        let prediction = self.predict_merge(&head, &up_oid);
        match pull_action(&prediction) {
            PullAction::UpToDate => self.info("Already up to date".to_string()),
            PullAction::FastForward => {
                let previous = self.repo.head_oid();
                match self.cli.merge_ff(&up_name) {
                    Ok(()) => {
                        if let Some(previous) = previous {
                            self.last_action = Some(UndoableAction::Merged { previous });
                        }
                        self.reload();
                        self.celebrate(Event::Pull);
                        self.info(format!("Pulled — fast-forwarded to {up_name}"));
                    }
                    Err(error) => self.fail(error.to_string()),
                }
            }
            PullAction::Choose => self.open_join_for(Some(up_name), up_oid),
        }
    }

    fn ask_stash_message(&mut self) {
        if self.status.is_empty() {
            self.info("Nothing to stash".to_string());
            return;
        }
        self.stash_message = Some(String::new());
    }

    fn stash_save(&mut self) {
        let message = self.stash_message.take().unwrap_or_default();
        if self.status.is_empty() {
            self.info("Nothing to stash".to_string());
            return;
        }
        let message = message.trim();
        let message = (!message.is_empty()).then_some(message);
        match self.cli.stash_save(message) {
            Ok(()) => {
                self.last_action = Some(UndoableAction::Stashed);
                self.reload();
                self.info("Stashed working changes".to_string());
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn toggle_stashes(&mut self) {
        if self.stash.is_some() {
            self.close_stash();
            return;
        }
        self.stash_all = self.cli.stash_list();
        self.stash_filter = None;
        self.stash = Some(StashView::opening(self.stash_all.clone()));
        self.sync_stash_files();
    }

    fn close_stash(&mut self) {
        if let Some(view) = &mut self.stash {
            view.panel.close();
        }
    }

    fn refresh_stash(&mut self) {
        self.stash_all = self.cli.stash_list();
        self.apply_stash_filter();
        self.sync_stash_files();
    }

    fn start_stash_filter(&mut self) {
        if self.stash.is_none() {
            return;
        }
        self.stash_filter = Some(TextFilter {
            query: String::new(),
            editing: true,
        });
    }

    fn stash_filter_input(&mut self, character: char) {
        if let Some(filter) = &mut self.stash_filter {
            filter.query.push(character);
        }
        self.apply_stash_filter();
        self.sync_stash_files();
    }

    fn stash_filter_backspace(&mut self) {
        if let Some(filter) = &mut self.stash_filter {
            filter.query.pop();
        }
        self.apply_stash_filter();
        self.sync_stash_files();
    }

    fn stash_filter_submit(&mut self) {
        if let Some(filter) = &mut self.stash_filter {
            if filter.query.is_empty() {
                self.stash_filter = None;
            } else {
                filter.editing = false;
            }
        }
    }

    fn apply_stash_filter(&mut self) {
        let query = self
            .stash_filter
            .as_ref()
            .map(|filter| filter.query.clone())
            .unwrap_or_default();
        let entries = if query.is_empty() {
            self.stash_all.clone()
        } else {
            let haystack: Vec<String> = self
                .stash_all
                .iter()
                .map(|entry| {
                    format!(
                        "{} {}",
                        entry.message.text(),
                        entry.branch.clone().unwrap_or_default()
                    )
                })
                .collect();
            let refs: Vec<&str> = haystack.iter().map(String::as_str).collect();
            fuzzy_filter(&query, &refs)
                .into_iter()
                .map(|index| self.stash_all[index].clone())
                .collect()
        };
        if let Some(view) = &mut self.stash {
            view.panel.refresh(entries);
            view.panel.selected = 0;
        }
    }

    pub fn stash_filter_query(&self) -> Option<&str> {
        self.stash_filter
            .as_ref()
            .map(|filter| filter.query.as_str())
    }

    pub fn stash_filter_editing(&self) -> bool {
        self.stash_filter
            .as_ref()
            .is_some_and(|filter| filter.editing)
    }

    fn stash_move(&mut self, delta: isize) {
        let Some(view) = &mut self.stash else {
            return;
        };
        match view.focus {
            StashFocus::Entries => {
                view.panel.move_selection(delta);
                self.sync_stash_files();
            }
            StashFocus::Files => {
                view.move_file(delta);
                self.sync_stash_diff();
            }
            StashFocus::Hunks => {
                if view.fullscreen {
                    view.move_hunk(delta);
                } else {
                    view.diff_scroll = shift_scroll(view.diff_scroll, delta);
                }
            }
        }
    }

    fn sync_stash_files(&mut self) {
        let Some(view) = &self.stash else {
            return;
        };
        let files = view
            .focused_entry()
            .map(|entry| self.stash_files(entry))
            .unwrap_or_default();
        if let Some(view) = &mut self.stash {
            view.files = files;
            view.file = 0;
            view.files_scroll = 0;
            view.hunk = 0;
            if view.files.is_empty() {
                view.focus = StashFocus::Entries;
            }
        }
        self.sync_stash_diff();
    }

    fn stash_files(&self, entry: &crate::stash::StashEntry) -> Vec<StashFile> {
        let Ok(oid) = Oid::from_str(&entry.oid) else {
            return Vec::new();
        };
        let untracked = self.repo.untracked_parent(oid);
        let mut files = Vec::new();
        for (id, is_untracked) in
            std::iter::once((oid, false)).chain(untracked.map(|id| (id, true)))
        {
            let stats = self.repo.change_stats(id);
            let Ok(changes) = self.repo.changed_files(id) else {
                continue;
            };
            for change in changes {
                files.push(StashFile {
                    stats: stats.get(&change.path).copied().unwrap_or_default(),
                    path: change.path,
                    status: change.status,
                    untracked: is_untracked,
                });
            }
        }
        files
    }

    fn sync_stash_diff(&mut self) {
        let Some(view) = &self.stash else {
            return;
        };
        let target = view
            .focused_entry()
            .zip(view.focused_file())
            .and_then(|(entry, file)| {
                let oid = Oid::from_str(&entry.oid).ok()?;
                let id = if file.untracked {
                    self.repo.untracked_parent(oid)?
                } else {
                    oid
                };
                Some((id, file.path.clone()))
            });
        let diff =
            target.and_then(|(id, path)| self.repo.commit_file_diff(id, &path).ok().flatten());
        if let Some(view) = &mut self.stash {
            view.diff = diff;
            if view.diff.is_none() && view.focus == StashFocus::Hunks {
                view.focus = StashFocus::Files;
            }
        }
    }

    fn focused_stash(&self) -> Option<usize> {
        self.stash
            .as_ref()
            .and_then(StashView::focused_entry)
            .map(|entry| entry.index)
    }

    fn focused_stash_conflict(&self, pop: bool) -> Option<StashConflict> {
        self.stash
            .as_ref()
            .and_then(StashView::focused_entry)
            .map(|entry| StashConflict {
                index: entry.index,
                message: entry.message.text().to_string(),
                pop,
            })
    }

    fn ask_stash_branch(&mut self) {
        let Some(index) = self.focused_stash() else {
            return;
        };
        self.branch_create = Some(BranchCreate {
            name: String::new(),
            start: BranchStart::Stash(index),
        });
    }

    fn stash_branch(&mut self, index: usize, name: &str) {
        let Some(entry) = self.focused_stash_conflict(true) else {
            return;
        };
        let result = self.cli.stash_branch(index, name);
        self.after_stash_restore(result, entry, &format!("Stash is now {name}"));
    }

    fn stash_restore_file(&mut self) {
        let Some(view) = &self.stash else {
            return;
        };
        if view.focus == StashFocus::Entries {
            return;
        }
        let Some((index, file)) = view
            .focused_entry()
            .map(|entry| entry.index)
            .zip(view.focused_file().cloned())
        else {
            return;
        };
        let snapshot = self
            .repo
            .snapshot_blob(&file.path)
            .map(|oid| oid.to_string());
        let result = self
            .cli
            .stash_restore_file(index, &file.path, file.untracked);
        let restored = result.is_ok();
        self.after_mutation_recording(
            result,
            UndoableAction::RestoredFromStash {
                path: file.path.clone(),
                snapshot,
            },
        );
        if restored {
            self.info(format!("Restored {} from stash@{{{index}}}", file.path));
        }
    }

    fn stash_pop(&mut self) {
        let Some(entry) = self.focused_stash_conflict(true) else {
            return;
        };
        let result = self.cli.stash_pop(entry.index);
        self.after_stash_restore(result, entry, "Popped stash");
    }

    fn stash_apply(&mut self) {
        let Some(entry) = self.focused_stash_conflict(false) else {
            return;
        };
        let result = self.cli.stash_apply(entry.index);
        self.after_stash_restore(result, entry, "Applied stash");
    }

    fn request_drop_stash(&mut self) {
        let Some(index) = self.focused_stash() else {
            return;
        };
        self.confirm = Some(Confirm {
            message: format!("Drop stash@{{{index}}}? (y/n)"),
            kind: ConfirmKind::DropStash { index },
        });
    }

    fn after_stash_mutation(&mut self, result: Result<(), MutationError>, ok_notice: &str) {
        self.reload();
        self.refresh_stash();
        match result {
            Ok(()) => self.info(ok_notice.to_string()),
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn show_restored_changes(&mut self) {
        if !self.has_wip() || self.on_wip {
            return;
        }
        self.on_wip = true;
        self.sync_peek();
    }

    fn after_stash_restore(
        &mut self,
        result: Result<(), MutationError>,
        entry: StashConflict,
        ok_notice: &str,
    ) {
        let Err(error) = result else {
            self.close_stash();
            self.after_stash_mutation(Ok(()), ok_notice);
            self.show_restored_changes();
            return;
        };
        let conflicts = self.repo.conflicted_files();
        if conflicts.is_empty() {
            self.after_stash_mutation(Err(error), ok_notice);
            return;
        }
        let count = conflicts.len();
        self.stash_conflict = Some(entry);
        self.stash = None;
        self.enter_conflict_browser(OpKind::Stash);
        self.reload();
        self.refresh_stash();
        self.fail(format!(
            "Stash conflicts in {count} file(s) — resolve, then [c]"
        ));
    }

    fn palette_input(&mut self, character: char) {
        if let Some(palette) = &mut self.palette {
            palette.query.push(character);
            palette.refilter();
        }
    }

    fn palette_backspace(&mut self) {
        if let Some(palette) = &mut self.palette {
            palette.query.pop();
            palette.refilter();
        }
    }

    fn palette_submit(&mut self) {
        let Some(palette) = self.palette.take() else {
            return;
        };
        let Some(&index) = palette.matches.get(palette.selected) else {
            return;
        };
        let action = COMMANDS[index].2.clone();
        self.update(action);
    }

    fn push(&mut self) {
        if self.remote_busy() {
            return;
        }
        let Some(branch) = self.meta.head_branch.clone() else {
            self.info("Detached HEAD — nothing to push".to_string());
            return;
        };
        match push_state(self.repo.head_tracking()) {
            PushState::UpToDate => self.info("Nothing to push".to_string()),
            PushState::Diverged { .. } => {
                self.confirm = Some(Confirm {
                    message: "Upstream has diverged — force-with-lease push? (y/n)".to_string(),
                    kind: ConfirmKind::ForcePush,
                });
            }
            PushState::NoUpstream => {
                self.spawn_push(move |cli| cli.push_set_upstream("origin", &branch));
            }
            PushState::Ahead(_) => self.spawn_push(|cli| cli.push()),
        }
    }

    fn spawn_push(
        &mut self,
        op: impl FnOnce(&GitCli) -> Result<(), MutationError> + Send + 'static,
    ) {
        self.spawn_remote("pushing", OnComplete::Push, op);
    }

    fn spawn_remote(
        &mut self,
        verb: &'static str,
        on_complete: OnComplete,
        op: impl FnOnce(&GitCli) -> Result<(), MutationError> + Send + 'static,
    ) {
        let workdir = self
            .repo
            .workdir()
            .unwrap_or_else(|| self.repo.git_dir())
            .to_path_buf();
        let askpass = self.askpass_config();
        self.askpass_used.clear();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut cli = GitCli::new(workdir);
            if let Some(config) = askpass {
                cli = cli.with_askpass(config);
            }
            let _ = tx.send(op(&cli));
        });
        self.notice = None;
        self.remote = Some(RemoteJob {
            verb,
            on_complete,
            spinner: 0,
            rx,
        });
    }

    pub fn poll_remote(&mut self) {
        let Some(job) = &self.remote else {
            return;
        };
        match job.rx.try_recv() {
            Ok(result) => {
                let on_complete = job.on_complete;
                self.remote = None;
                let cancelled = std::mem::take(&mut self.askpass_cancelled);
                match result {
                    Ok(()) => {
                        self.reload();
                        match on_complete {
                            OnComplete::Notice(message) => self.info(message.to_string()),
                            OnComplete::Push => {
                                self.celebrate(Event::Push);
                                self.info("Pushed".to_string());
                            }
                            OnComplete::Pull => self.pull_integrate(),
                        }
                    }
                    Err(error) => {
                        let text = error.to_string();
                        if cancelled {
                            self.info("Authentication cancelled".to_string());
                        } else if is_auth_failure(&text) {
                            self.forget_used_credentials();
                            self.fail(text);
                        } else if matches!(on_complete, OnComplete::Push)
                            && is_non_fast_forward(&text)
                        {
                            self.confirm = Some(Confirm {
                                message:
                                    "Push rejected — remote has moved. Force-with-lease? (y/n)"
                                        .to_string(),
                                kind: ConfirmKind::ForcePush,
                            });
                        } else {
                            self.fail(text);
                        }
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.remote = None;
                self.fail("Remote operation failed".to_string());
            }
        }
    }

    fn askpass_config(&mut self) -> Option<AskpassConfig> {
        let socket = self.ensure_askpass()?;
        let helper = std::env::current_exe().ok()?;
        Some(AskpassConfig { helper, socket })
    }

    fn ensure_askpass(&mut self) -> Option<PathBuf> {
        if let Some(socket) = &self.askpass_sock {
            return Some(socket.clone());
        }
        let server = AskpassServer::bind().ok()?;
        let socket = server.path().to_path_buf();
        self.askpass_sock = Some(socket.clone());
        self.askpass_requests = Some(server.into_requests());
        Some(socket)
    }

    pub fn poll_askpass(&mut self) {
        if self.password.is_some() {
            return;
        }
        let Some(requests) = &self.askpass_requests else {
            return;
        };
        let Ok(request) = requests.try_recv() else {
            return;
        };
        let prompt = parse_askpass_prompt(request.prompt());
        self.record_used_host(&prompt.host);
        if let Some(value) = self.credentials.get(&prompt.host, prompt.kind) {
            request.answer(value.to_string());
            return;
        }
        self.password = Some(PasswordPrompt {
            kind: prompt.kind,
            host: prompt.host,
            input: String::new(),
            request,
        });
    }

    fn record_used_host(&mut self, host: &str) {
        if !self.askpass_used.iter().any(|used| used == host) {
            self.askpass_used.push(host.to_string());
        }
    }

    fn forget_used_credentials(&mut self) {
        for host in std::mem::take(&mut self.askpass_used) {
            self.credentials.invalidate(&host);
        }
    }

    fn password_input(&mut self, character: char) {
        if let Some(prompt) = &mut self.password {
            prompt.input.push(character);
        }
    }

    fn password_backspace(&mut self) {
        if let Some(prompt) = &mut self.password {
            prompt.input.pop();
        }
    }

    fn password_submit(&mut self) {
        let Some(prompt) = self.password.take() else {
            return;
        };
        self.credentials
            .store(&prompt.host, prompt.kind, prompt.input.clone());
        prompt.request.answer(prompt.input);
    }

    fn password_cancel(&mut self) {
        let Some(prompt) = self.password.take() else {
            return;
        };
        prompt.request.answer(String::new());
        self.askpass_cancelled = true;
    }

    fn open_or_expand(&mut self) {
        if let Some(view) = &mut self.stash {
            view.fullscreen = !view.fullscreen;
            view.diff_scroll = 0;
            return;
        }
        if let Some(view) = &mut self.working {
            view.fullscreen = !view.fullscreen;
        } else if self.on_wip {
            if self.has_wip() {
                self.panel = None;
                self.working = Some(WorkingView::opening());
                self.sync_focus_diff();
            }
        } else if self.panel.as_ref().is_some_and(|panel| !panel.fullscreen) {
            self.expand_commit_diff();
        } else {
            self.open_panel();
        }
    }

    fn open_panel(&mut self) {
        let Some(id) = self.selected_commit().map(|commit| commit.id) else {
            return;
        };
        self.working = None;
        let (changed_files, pending) = match self.repo.cached_changed_files(id) {
            Some(files) => (files, false),
            None => (Vec::new(), true),
        };
        self.peek_debounce = pending.then_some(PEEK_DEBOUNCE);
        match &mut self.panel {
            Some(panel) => {
                panel.commit_index = self.selected;
                panel.changed_files = changed_files;
                panel.target = 1.0;
                panel.fullscreen = false;
                panel.file = 0;
                panel.diff = None;
                panel.split = false;
                panel.diff_scroll = 0;
                panel.diff_focused = false;
            }
            None => {
                self.panel = Some(Panel {
                    commit_index: self.selected,
                    changed_files,
                    progress: 0.0,
                    target: 1.0,
                    fullscreen: false,
                    file: 0,
                    diff: None,
                    split: false,
                    diff_scroll: 0,
                    diff_hscroll: 0,
                    files_scroll: 0,
                    diff_focused: false,
                });
            }
        }
    }

    fn fill_peek(&mut self) {
        let Some(index) = self.panel.as_ref().map(|panel| panel.commit_index) else {
            return;
        };
        let Some(id) = self.commits.get(index).map(|commit| commit.id) else {
            return;
        };
        let files = self.repo.changed_files(id).unwrap_or_default();
        if let Some(panel) = &mut self.panel {
            panel.changed_files = files;
        }
    }

    fn flush_peek(&mut self) {
        if self.peek_debounce.take().is_some() {
            self.fill_peek();
        }
    }

    fn expand_commit_diff(&mut self) {
        self.flush_peek();
        if let Some(panel) = &mut self.panel {
            panel.fullscreen = true;
            panel.file = 0;
            panel.diff_scroll = 0;
            panel.diff_focused = false;
        }
        self.sync_commit_diff();
    }

    fn sync_commit_diff(&mut self) {
        let Some(panel) = self.panel.as_ref() else {
            return;
        };
        let commit_id = self.commits.get(panel.commit_index).map(|commit| commit.id);
        let path = panel
            .changed_files
            .get(panel.file)
            .map(|file| file.path.clone());
        let diff = match (commit_id, path) {
            (Some(id), Some(path)) => self.repo.commit_file_diff(id, &path).ok().flatten(),
            _ => None,
        };
        if let Some(panel) = &mut self.panel {
            panel.diff = diff;
        }
    }

    fn commit_diff_move(&mut self, delta: isize) {
        if let Some(panel) = &mut self.panel {
            panel.file = slide::clamp_index(panel.file, delta, panel.changed_files.len());
            panel.diff_scroll = 0;
            panel.diff_hscroll = 0;
        }
        self.sync_commit_diff();
    }

    fn dismiss(&mut self) {
        if !self.dismiss_layer() {
            self.exit_submodule();
        }
    }

    fn dismiss_layer(&mut self) -> bool {
        if self.password.is_some() {
            self.password_cancel();
        } else if self.focus_name.is_some() {
            self.focus_name = None;
        } else if self.stash_filter.is_some() {
            self.stash_filter = None;
            self.apply_stash_filter();
        } else if self.branch_filter.is_some() {
            self.branch_filter = None;
            self.apply_branch_filter();
        } else if self.alert.is_some() {
            self.alert = None;
        } else if self.palette.is_some() {
            self.palette = None;
        } else if self.stash_message.is_some() {
            self.stash_message = None;
        } else if self.commit.is_some() {
            self.commit = None;
        } else if self.branch_create.is_some() {
            self.branch_create = None;
        } else if self.confirm.is_some() {
            self.confirm = None;
        } else if self.help_visible {
            self.help_visible = false;
        } else if let Some(panel) = &mut self.focus_panel {
            panel.close();
        } else if let Some(panel) = &mut self.submodule_panel {
            panel.close();
        } else if let Some(view) = &mut self.stash {
            if view.focus != StashFocus::Entries {
                view.focus = StashFocus::Entries;
            } else if view.fullscreen {
                view.fullscreen = false;
            } else {
                view.panel.close();
            }
        } else if let Some(menu) = &mut self.join {
            menu.panel.close();
        } else if let Some(panel) = &mut self.branch {
            panel.close();
        } else if let Some(view) = &mut self.working {
            if view.focus == Focus::Hunks {
                view.focus = Focus::Files;
            } else if view.fullscreen {
                view.fullscreen = false;
            } else {
                view.target = 0.0;
            }
        } else if let Some(panel) = &mut self.panel {
            if panel.fullscreen {
                panel.fullscreen = false;
            } else {
                panel.target = 0.0;
            }
        } else {
            return false;
        }
        true
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
        let mode = discard_mode(&file);
        let scope = match mode {
            DiscardMode::Head => "staged changes to",
            _ => "changes to",
        };
        self.confirm = Some(Confirm {
            message: format!("Discard {scope} {}? (y/n)", file.path),
            kind: ConfirmKind::Discard {
                path: file.path,
                mode,
            },
        });
    }

    fn confirm_yes(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        match confirm.kind {
            ConfirmKind::Discard { path, mode } => {
                let snapshot = self.repo.snapshot_blob(&path);
                let result = match mode {
                    DiscardMode::Untracked => self.cli.remove_untracked(&path),
                    DiscardMode::Head => self.cli.restore_from_head(std::slice::from_ref(&path)),
                    DiscardMode::Worktree => self.cli.discard_file(&path),
                };
                self.finish_discard(result, path, snapshot);
            }
            ConfirmKind::DiscardHunk { path, patch } => {
                let snapshot = self.repo.snapshot_blob(&path);
                let result = self.cli.discard_hunk(&patch);
                self.finish_discard(result, path, snapshot);
            }
            ConfirmKind::DeleteBranch {
                name,
                oid,
                upstream,
            } => match self.cli.delete_branch(&name) {
                Ok(()) => self.finish_branch_delete(name, oid, upstream),
                Err(_) => {
                    self.confirm = Some(Confirm {
                        message: format!("'{name}' is not fully merged. Force delete? (y/n)"),
                        kind: ConfirmKind::ForceDeleteBranch {
                            name,
                            oid,
                            upstream,
                        },
                    });
                }
            },
            ConfirmKind::ForceDeleteBranch {
                name,
                oid,
                upstream,
            } => match self.cli.force_delete_branch(&name) {
                Ok(()) => self.finish_branch_delete(name, oid, upstream),
                Err(error) => self.fail(error.to_string()),
            },
            ConfirmKind::DeleteRemoteBranch {
                remote,
                remote_branch,
            } => self.spawn_delete_remote(remote, remote_branch),
            ConfirmKind::ForcePush => {
                self.spawn_push(|cli| cli.push_force_with_lease());
            }
            ConfirmKind::DropStash { index } => {
                let result = self.cli.stash_drop(index);
                self.after_stash_mutation(result, "Dropped stash");
            }
            ConfirmKind::AbortStashConflict => self.abort_stash_conflict(),
            ConfirmKind::DeleteFocus { name } => {
                focus::remove(&mut self.focus_sets, &name);
                self.write_focus_sets();
                self.refresh_focus_panel();
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
            self.info("Nothing staged to commit".to_string());
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
            self.info("Empty commit message".to_string());
            self.commit = Some(editor);
            return;
        }
        if let Some(op) = editor.finish {
            self.finish_conflict_commit(op, &editor.message);
            return;
        }
        let result = self.cli.commit(&editor.message);
        let committed = result.is_ok();
        self.after_mutation_recording(result, UndoableAction::Committed);
        self.reload();
        if committed {
            self.celebrate(Event::Commit);
        }
    }

    fn toggle_focus_back(&mut self) {
        match &mut self.stash {
            Some(view) => view.rotate_focus_back(),
            None => self.toggle_focus(),
        }
    }

    fn toggle_focus(&mut self) {
        if let Some(view) = &mut self.stash {
            view.rotate_focus();
            return;
        }
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
                        view.diff_scroll = 0;
                    }
                }
                Focus::Hunks => {
                    view.focus = Focus::Files;
                    view.diff_scroll = 0;
                }
            }
        } else if let Some(panel) = self.panel.as_mut().filter(|panel| panel.fullscreen) {
            if panel.diff_focused {
                panel.diff_focused = false;
            } else if panel.diff.is_some() {
                panel.diff_focused = true;
            }
        }
    }

    fn toggle_diff_view(&mut self) {
        if let Some(view) = &mut self.stash {
            view.split = !view.split;
            view.diff_scroll = 0;
            view.diff_hscroll = 0;
            return;
        }
        if let Some(view) = &mut self.working {
            view.split = !view.split;
            view.diff_scroll = 0;
            view.diff_hscroll = 0;
        } else if let Some(panel) = &mut self.panel
            && panel.fullscreen
        {
            panel.split = !panel.split;
            panel.diff_scroll = 0;
            panel.diff_hscroll = 0;
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
        match result {
            Ok(()) => self.notice = None,
            Err(error) => self.fail(error.to_string()),
        }
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
            self.info("Nothing to undo".to_string());
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
            InversePlan::UndoStashRestore { path, snapshot } => {
                let undone = match snapshot {
                    Some(snapshot) => {
                        Oid::from_str(&snapshot).is_ok_and(|oid| self.repo.restore_blob(&path, oid))
                    }
                    None => self.repo.remove_workdir_file(&path),
                };
                (undone, format!("Undid restore of {path}"))
            }
            InversePlan::ReflogSoftReset => (
                self.cli.reset_soft_previous().is_ok(),
                "Undid commit".to_string(),
            ),
            InversePlan::Checkout(target) => (
                self.cli.checkout(&target).is_ok(),
                format!("Undid checkout — back on {target}"),
            ),
            InversePlan::DropBranch { name, back_to } => (
                self.cli.checkout(&back_to).is_ok() && self.cli.delete_branch(&name).is_ok(),
                format!("Undid create of {name}"),
            ),
            InversePlan::RestoreBranch { name, oid } => (
                self.cli.create_branch_at(&name, &oid).is_ok(),
                format!("Undid delete of {name}"),
            ),
            InversePlan::ResetKeep(oid) => (
                self.cli.reset_keep(&oid).is_ok(),
                "Undid integration".to_string(),
            ),
            InversePlan::StashPop => (self.cli.stash_pop(0).is_ok(), "Undid stash".to_string()),
            InversePlan::CheckoutInSubmodule { path, oid } => (
                GitCli::new(PathBuf::from(&path))
                    .checkout_detached(&oid)
                    .is_ok(),
                "Undid submodule update".to_string(),
            ),
        };
        if ok {
            self.info(description);
        } else {
            self.fail("Undo failed".to_string());
            self.last_action = Some(action);
        }
        self.reload();
    }

    fn load_branch_entries(&self) -> Vec<BranchEntry> {
        let inputs = self.repo.branches().unwrap_or_default();
        branch_list(&inputs, self.meta.head_branch.as_deref())
    }

    fn close_branch_panel(&mut self) {
        if let Some(panel) = &mut self.branch {
            panel.close();
        }
    }

    fn toggle_branches(&mut self) {
        if self.branch.is_some() {
            self.close_branch_panel();
            return;
        }
        self.branch_all = self.load_branch_entries();
        self.branch = Some(BranchPanel::opening(self.branch_all.clone()));
    }

    fn checkout(&mut self) {
        if self.branch.is_some() {
            self.checkout_focused_branch();
        } else {
            self.checkout_selected_commit();
        }
    }

    fn focused_branch_name(&self) -> Option<String> {
        self.branch
            .as_ref()
            .and_then(BranchPanel::focused)
            .map(|entry| entry.name.clone())
    }

    fn branch_names(&self) -> Vec<String> {
        self.branch_all
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    fn toggle_branch_visibility(&mut self) {
        let Some(name) = self.focused_branch_name() else {
            return;
        };
        self.visibility.toggle(&name);
        self.reload();
    }

    fn solo_branch(&mut self) {
        let Some(name) = self.focused_branch_name() else {
            return;
        };
        let names = self.branch_names();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        self.visibility.solo(&name, &refs);
        self.reload();
    }

    fn pin_branch(&mut self) {
        let Some(name) = self.focused_branch_name() else {
            return;
        };
        self.visibility.pin(&name);
        self.reload();
    }

    fn start_branch_filter(&mut self) {
        if self.branch.is_none() {
            return;
        }
        self.branch_filter = Some(TextFilter {
            query: String::new(),
            editing: true,
        });
    }

    fn branch_filter_input(&mut self, character: char) {
        if let Some(filter) = &mut self.branch_filter {
            filter.query.push(character);
        }
        self.requery_branch_filter();
    }

    fn branch_filter_backspace(&mut self) {
        if let Some(filter) = &mut self.branch_filter {
            filter.query.pop();
        }
        self.requery_branch_filter();
    }

    fn requery_branch_filter(&mut self) {
        self.apply_branch_filter();
        if let Some(panel) = &mut self.branch {
            panel.selected = 0;
        }
    }

    fn branch_filter_submit(&mut self) {
        if let Some(filter) = &mut self.branch_filter {
            if filter.query.is_empty() {
                self.branch_filter = None;
            } else {
                filter.editing = false;
            }
        }
    }

    fn apply_branch_filter(&mut self) {
        let query = self
            .branch_filter
            .as_ref()
            .map(|filter| filter.query.clone())
            .unwrap_or_default();
        let entries = if query.is_empty() {
            self.branch_all.clone()
        } else {
            let names = self.branch_names();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            fuzzy_filter(&query, &refs)
                .into_iter()
                .map(|index| self.branch_all[index].clone())
                .collect()
        };
        if let Some(panel) = &mut self.branch {
            panel.refresh(entries);
        }
    }

    fn toggle_focus_panel(&mut self) {
        if self.focus_panel.is_some() {
            self.close_focus_panel();
            return;
        }
        self.focus_panel = Some(FocusPanel::opening(self.focus_sets.clone()));
    }

    fn close_focus_panel(&mut self) {
        if let Some(panel) = &mut self.focus_panel {
            panel.close();
        }
    }

    fn start_save_focus(&mut self) {
        if self.focus_panel.is_some() {
            self.focus_name = Some(String::new());
        }
    }

    fn focus_name_input(&mut self, character: char) {
        if let Some(name) = &mut self.focus_name {
            name.push(character);
        }
    }

    fn focus_name_backspace(&mut self) {
        if let Some(name) = &mut self.focus_name {
            name.pop();
        }
    }

    fn focus_name_submit(&mut self) {
        let Some(name) = self.focus_name.take() else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let state = self.visibility.snapshot();
        focus::upsert(&mut self.focus_sets, FocusSet { name, state });
        self.write_focus_sets();
        self.refresh_focus_panel();
    }

    fn refresh_focus_panel(&mut self) {
        let entries = self.focus_sets.clone();
        if let Some(panel) = &mut self.focus_panel {
            panel.refresh(entries);
        }
    }

    fn activate_focus(&mut self) {
        let Some(set) = self
            .focus_panel
            .as_ref()
            .and_then(FocusPanel::focused)
            .cloned()
        else {
            return;
        };
        self.visibility.restore(set.state);
        self.close_focus_panel();
        self.reload();
    }

    fn request_delete_focus(&mut self) {
        let Some(name) = self
            .focus_panel
            .as_ref()
            .and_then(FocusPanel::focused)
            .map(|set| set.name.clone())
        else {
            return;
        };
        self.confirm = Some(Confirm {
            message: format!("Delete focus set '{name}'? (y/n)"),
            kind: ConfirmKind::DeleteFocus { name },
        });
    }

    fn toggle_submodule_panel(&mut self) {
        if self.submodule_panel.is_some() {
            self.close_submodule_panel();
            return;
        }
        self.submodule_panel = Some(SubmodulePanel::opening(self.repo.submodules()));
    }

    fn close_submodule_panel(&mut self) {
        if let Some(panel) = &mut self.submodule_panel {
            panel.close();
        }
    }

    fn current_workdir(&self) -> PathBuf {
        self.repo
            .workdir()
            .unwrap_or_else(|| self.repo.git_dir())
            .to_path_buf()
    }

    fn enter_submodule(&mut self) {
        let Some(sub) = self
            .submodule_panel
            .as_ref()
            .and_then(SubmodulePanel::focused)
            .cloned()
        else {
            return;
        };
        if !sub.initialized() {
            self.info(format!("Submodule '{}' is not initialized", sub.name));
            return;
        }
        let current = self.current_workdir();
        let sub_workdir = current.join(&sub.path);
        let mut breadcrumb = self.breadcrumb.clone();
        breadcrumb.push(current);
        match App::open(&sub_workdir) {
            Ok(mut app) => {
                app.breadcrumb = breadcrumb;
                self.swap_with_transition(app, NavDirection::Push);
            }
            Err(_) => self.fail(format!("Could not open submodule '{}'", sub.name)),
        }
    }

    fn swap_with_transition(&mut self, mut app: App, direction: NavDirection) {
        app.intensity = self.intensity;
        app.refresh_parent_sync();
        let outgoing = std::mem::replace(self, app);
        self.nav_transition = Some(NavTransition {
            outgoing: Box::new(outgoing),
            direction,
            progress: 0.0,
        });
    }

    fn refresh_parent_sync(&mut self) {
        self.parent_sync = self.breadcrumb.last().and_then(|parent| {
            let repo = Repo::discover(parent).ok()?;
            let workdir = self.current_workdir().canonicalize().ok()?;
            repo.submodules()
                .into_iter()
                .find(|sub| {
                    parent
                        .join(&sub.path)
                        .canonicalize()
                        .is_ok_and(|path| path == workdir)
                })
                .map(|sub| sub.sync)
        });
    }

    fn init_submodule(&mut self) {
        let Some(sub) = self
            .submodule_panel
            .as_ref()
            .and_then(SubmodulePanel::focused)
            .cloned()
        else {
            return;
        };
        if sub.initialized() {
            self.info(format!("Submodule '{}' is already initialized", sub.name));
            return;
        }
        let result = self.cli.submodule_init(&sub.path);
        let succeeded = result.is_ok();
        self.after_mutation(result);
        if succeeded {
            self.info(format!("Initialized submodule '{}'", sub.name));
        }
        self.refresh_submodule_panel();
    }

    fn update_submodule(&mut self) {
        let Some(sub) = self
            .submodule_panel
            .as_ref()
            .and_then(SubmodulePanel::focused)
            .cloned()
        else {
            return;
        };
        if sub.sync != SyncState::Drifted {
            self.info(format!(
                "Submodule '{}' is already at the recorded commit",
                sub.name
            ));
            return;
        }
        let previous = sub.checked_out.clone().unwrap_or_default();
        let result = self.cli.submodule_update(&sub.path);
        let succeeded = result.is_ok();
        self.after_mutation_recording(
            result,
            UndoableAction::SubmoduleUpdated {
                path: self
                    .current_workdir()
                    .join(&sub.path)
                    .to_string_lossy()
                    .into_owned(),
                previous,
            },
        );
        if succeeded {
            self.info(format!(
                "Updated submodule '{}' to the recorded commit",
                sub.name
            ));
        }
        self.refresh_submodule_panel();
    }

    fn refresh_submodule_panel(&mut self) {
        if let Some(panel) = &mut self.submodule_panel {
            panel.refresh(self.repo.submodules());
        }
    }

    fn exit_submodule(&mut self) {
        if self.nav_transition.is_some() {
            return;
        }
        let mut breadcrumb = self.breadcrumb.clone();
        let Some(parent) = breadcrumb.pop() else {
            return;
        };
        let child_workdir = self.current_workdir().canonicalize().ok();
        match App::open(&parent) {
            Ok(mut app) => {
                app.breadcrumb = breadcrumb;
                app.reopen_submodule_panel(child_workdir.as_deref());
                self.swap_with_transition(app, NavDirection::Pop);
            }
            Err(_) => self.fail("Could not return to the parent repository".to_string()),
        }
    }

    fn reopen_submodule_panel(&mut self, child_workdir: Option<&Path>) {
        let workdir = self.current_workdir();
        let mut panel = SubmodulePanel::opening(self.repo.submodules());
        if let Some(child) = child_workdir
            && let Some(index) = panel.entries.iter().position(|sub| {
                workdir
                    .join(&sub.path)
                    .canonicalize()
                    .is_ok_and(|path| path == child)
            })
        {
            panel.selected = index;
        }
        self.submodule_panel = Some(panel);
    }

    fn checkout_focused_branch(&mut self) {
        let Some(entry) = self.branch.as_ref().and_then(BranchPanel::focused) else {
            return;
        };
        if entry.remote.is_some() {
            let remote_ref = entry.name.clone();
            let local = split_remote_ref(&remote_ref)
                .map(|(_, branch)| branch)
                .unwrap_or_else(|| remote_ref.clone());
            self.perform_checkout_track(local, remote_ref);
            return;
        }
        if entry.is_head {
            self.close_branch_panel();
            return;
        }
        self.perform_checkout(entry.name.clone(), false);
    }

    fn checkout_selected_commit(&mut self) {
        if self.on_wip {
            return;
        }
        let Some(commit) = self.selected_commit() else {
            return;
        };
        let id = commit.id;
        match self.local_branch_at(id) {
            Some(name) => self.perform_checkout(name, false),
            None => self.perform_checkout(id.to_string(), true),
        }
    }

    fn local_branch_at(&self, id: Oid) -> Option<String> {
        self.meta
            .badges
            .get(&id)?
            .iter()
            .find(|badge| {
                matches!(
                    badge.kind,
                    BadgeKind::LocalBranch | BadgeKind::CurrentBranch
                )
            })
            .map(|badge| badge.label.clone())
    }

    fn perform_checkout(&mut self, target: String, detached: bool) {
        let previous = self.repo.head_ref();
        let result = if detached {
            self.cli.checkout_detached(&target)
        } else {
            self.cli.switch_branch(&target)
        };
        self.finish_checkout(result, target, previous);
    }

    fn perform_checkout_track(&mut self, local: String, remote_ref: String) {
        let previous = self.repo.head_ref();
        let result = self.cli.switch_create_track(&local, &remote_ref);
        self.finish_checkout(result, local, previous);
    }

    fn finish_checkout(
        &mut self,
        result: Result<(), MutationError>,
        target: String,
        previous: Option<String>,
    ) {
        match result {
            Ok(()) => {
                if let Some(previous) = previous.filter(|previous| *previous != target) {
                    self.last_action = Some(UndoableAction::CheckedOut { previous });
                }
                self.close_branch_panel();
                self.reload();
                self.celebrate(Event::Checkout);
                let landing = self.landing_notice();
                self.info(landing);
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn celebrate(&mut self, event: Event) {
        if !should_celebrate(event, self.intensity) {
            return;
        }
        let Some(oid) = self.repo.head_oid() else {
            return;
        };
        self.celebration = Some(Celebration {
            progress: 1.0,
            oid,
            sparkle: self.intensity == Intensity::Full,
        });
    }

    fn cycle_celebrations(&mut self) {
        self.intensity = next_intensity(self.intensity);
        self.info(format!("Celebrations: {}", self.intensity.label()));
    }

    fn landing_notice(&self) -> String {
        match &self.meta.head_branch {
            Some(branch) => format!("On {branch}"),
            None => {
                let short: String = self
                    .repo
                    .head_ref()
                    .map(|head| head.chars().take(7).collect())
                    .unwrap_or_default();
                format!("Detached HEAD at {short}")
            }
        }
    }

    fn close_join_menu(&mut self) {
        if let Some(menu) = &mut self.join {
            menu.panel.close();
        }
    }

    fn open_join(&mut self) {
        let source = if let Some(entry) = self.branch.as_ref().and_then(BranchPanel::focused) {
            self.repo
                .branch_tip(&entry.name)
                .map(|oid| (Some(entry.name.clone()), oid))
        } else if !self.on_wip {
            self.selected_commit().map(|commit| {
                let name = self.local_branch_at(commit.id);
                (name, commit.id.to_string())
            })
        } else {
            None
        };
        let Some((source_name, source_oid)) = source else {
            return;
        };
        self.open_join_for(source_name, source_oid);
    }

    fn build_join_menu(
        &self,
        source_name: Option<String>,
        source_oid: String,
        head: &str,
    ) -> JoinMenu {
        let merge = self.predict_merge(head, &source_oid);
        let cherry = self.predict_cherry_pick(head, &source_oid);
        let mut options = Vec::new();
        let mut conflict_files = Vec::new();

        if source_name.is_some() {
            options.push(JoinOption {
                strategy: JoinStrategy::FastForward,
                label: "Fast-forward".to_string(),
                note: fast_forward_note(&merge),
                enabled: matches!(merge, MergePrediction::FastForward),
            });
            options.push(JoinOption {
                strategy: JoinStrategy::MergeCommit,
                label: "Merge commit".to_string(),
                note: merge_note(&merge),
                enabled: !matches!(merge, MergePrediction::UpToDate),
            });
            options.push(JoinOption {
                strategy: JoinStrategy::CherryPick,
                label: "Cherry-pick tip".to_string(),
                note: cherry_pick_note(&cherry),
                enabled: matches!(
                    cherry,
                    MergePrediction::Clean | MergePrediction::Conflicts { .. }
                ),
            });
            let attached = self.repo.head_branch().is_some();
            let ahead = self.repo.outgoing_count(head, &source_oid);
            options.push(JoinOption {
                strategy: JoinStrategy::Rebase,
                label: "Rebase onto".to_string(),
                note: rebase_note(&merge, ahead, attached),
                enabled: attached && !matches!(merge, MergePrediction::UpToDate),
            });
            if let MergePrediction::Conflicts { files } = &merge {
                conflict_files = files.clone();
            }
        } else {
            options.push(JoinOption {
                strategy: JoinStrategy::CherryPick,
                label: "Cherry-pick".to_string(),
                note: cherry_pick_note(&cherry),
                enabled: matches!(
                    cherry,
                    MergePrediction::Clean | MergePrediction::Conflicts { .. }
                ),
            });
            if let MergePrediction::Conflicts { files } = &cherry {
                conflict_files = files.clone();
            }
        }

        let title = match &source_name {
            Some(name) => format!("Join {name} into HEAD"),
            None => format!("Cherry-pick {} onto HEAD", short_oid(&source_oid)),
        };
        let incoming = self.repo.incoming_count(head, &source_oid);
        let summary = join_summary(&merge, source_name.is_some(), incoming);

        JoinMenu {
            title,
            source_name,
            source_oid,
            panel: SlidePanel::opening(options),
            conflict_files,
            summary,
        }
    }

    fn predict_merge(&self, head: &str, source: &str) -> MergePrediction {
        let ancestry = Ancestry {
            source_in_head: self.repo.is_ancestor(source, head),
            head_in_source: self.repo.is_ancestor(head, source),
        };
        let merge_tree = self.cli.merge_tree(head, source, None);
        classify(ancestry, &merge_tree)
    }

    fn predict_cherry_pick(&self, head: &str, commit: &str) -> MergePrediction {
        let base = self.repo.commit_parent(commit);
        let merge_tree = self.cli.merge_tree(head, commit, base.as_deref());
        let ancestry = Ancestry {
            source_in_head: self.repo.is_ancestor(commit, head),
            head_in_source: false,
        };
        classify(ancestry, &merge_tree)
    }

    fn execute_join(&mut self) {
        let Some(menu) = self.join.as_ref() else {
            return;
        };
        let Some(option) = menu.panel.focused() else {
            return;
        };
        if !option.enabled {
            self.info(format!("Not available: {}", option.note));
            return;
        }
        let strategy = option.strategy;
        let source_ref = menu
            .source_name
            .clone()
            .unwrap_or_else(|| menu.source_oid.clone());
        let source_oid = menu.source_oid.clone();

        if strategy == JoinStrategy::Rebase {
            self.execute_rebase(&source_ref);
            return;
        }

        let previous = self.repo.head_oid();
        let (result, label) = match strategy {
            JoinStrategy::FastForward => (self.cli.merge_ff(&source_ref), "Fast-forwarded"),
            JoinStrategy::MergeCommit => (self.cli.merge_no_ff(&source_ref), "Merged"),
            JoinStrategy::CherryPick => (self.cli.cherry_pick(&source_oid), "Cherry-picked"),
            JoinStrategy::Rebase => unreachable!(),
        };

        match result {
            Ok(()) => {
                if let Some(previous) = previous {
                    self.last_action = Some(match strategy {
                        JoinStrategy::CherryPick => UndoableAction::CherryPicked { previous },
                        _ => UndoableAction::Merged { previous },
                    });
                }
                self.close_join_menu();
                self.reload();
                self.celebrate(if strategy == JoinStrategy::CherryPick {
                    Event::CherryPick
                } else {
                    Event::Merge
                });
                self.info(label.to_string());
            }
            Err(error) => match self.repo.state_op() {
                Some(op) => self.enter_conflict_browser(op),
                None => self.fail(error.to_string()),
            },
        }
    }

    fn execute_rebase(&mut self, target: &str) {
        match self.cli.rebase(target) {
            Ok(()) => self.finish_rebase(),
            Err(error) => match self.repo.state_op() {
                Some(op) => self.enter_conflict_browser(op),
                None => self.fail(error.to_string()),
            },
        }
    }

    fn finish_rebase(&mut self) {
        self.record_rebase_undo();
        self.close_join_menu();
        self.conflict = None;
        self.reload();
        self.celebrate(Event::Rebase);
        self.info("Rebased".to_string());
    }

    fn record_rebase_undo(&mut self) {
        if let Some(previous) = self.repo.orig_head() {
            self.last_action = Some(UndoableAction::Rebased { previous });
        }
    }

    fn continue_rebase(&mut self) {
        let _ = self.cli.rebase_continue();
        self.resume_or_finish_rebase();
    }

    fn skip_rebase(&mut self) {
        if self.conflict.as_ref().map(|browser| browser.op) != Some(OpKind::Rebase) {
            return;
        }
        let _ = self.cli.rebase_skip();
        self.resume_or_finish_rebase();
    }

    fn resume_or_finish_rebase(&mut self) {
        if self.repo.state_op() == Some(OpKind::Rebase) {
            self.refresh_conflict_files();
        } else {
            self.finish_rebase();
        }
    }

    fn load_conflict_files(&self) -> Vec<ConflictFile> {
        self.repo
            .conflicted_files()
            .into_iter()
            .filter_map(|path| {
                self.repo
                    .read_workdir_file(&path)
                    .map(|content| ConflictFile::new(path, &content))
            })
            .collect()
    }

    fn detect_conflict(&mut self) {
        if self.conflict.is_some() {
            return;
        }
        let Some(op) = self.pending_op() else {
            return;
        };
        let files = self.load_conflict_files();
        if !files.is_empty() {
            let progress = self.repo.rebase_progress();
            self.conflict = Some(ConflictBrowser::new(op, files, progress));
        }
    }

    fn pending_op(&self) -> Option<OpKind> {
        self.repo.state_op().or_else(|| {
            (self.has_conflicts() && self.repo.is_clean_state()).then_some(OpKind::Stash)
        })
    }

    fn has_conflicts(&self) -> bool {
        self.status
            .unstaged
            .iter()
            .any(|file| file.status == CONFLICTED)
    }

    fn enter_conflict_browser(&mut self, op: OpKind) {
        let files = self.load_conflict_files();
        let progress = self.repo.rebase_progress();
        self.join = None;
        self.conflict = Some(ConflictBrowser::new(op, files, progress));
    }

    fn refresh_conflict_files(&mut self) {
        let files = self.load_conflict_files();
        let progress = self.repo.rebase_progress();
        if let Some(browser) = &mut self.conflict {
            browser.file = browser.file.min(files.len().saturating_sub(1));
            browser.block = 0;
            browser.files = files;
            browser.progress = progress;
        }
    }

    fn resolve_block(&mut self, side: Side) {
        let finalize = {
            let Some(browser) = &mut self.conflict else {
                return;
            };
            let Some(file) = browser.files.get_mut(browser.file) else {
                return;
            };
            if file.choices.is_empty() {
                return;
            }
            let block = browser.block.min(file.choices.len() - 1);
            file.choices[block] = Some(side);
            if file.is_resolved() {
                file.resolved_text().map(|text| (file.path.clone(), text))
            } else {
                browser.block = file
                    .choices
                    .iter()
                    .position(Option::is_none)
                    .unwrap_or(block);
                None
            }
        };
        if let Some((path, text)) = finalize {
            if self.repo.write_workdir_file(&path, &text) {
                let _ = self.cli.stage_file(&path);
            }
            self.refresh_conflict_files();
        }
    }

    fn continue_conflict(&mut self) {
        let Some(browser) = self.conflict.as_ref() else {
            return;
        };
        if !browser.files.is_empty() {
            self.info("Resolve every conflict first".to_string());
            return;
        }
        let op = browser.op;
        if op == OpKind::Rebase {
            self.continue_rebase();
            return;
        }
        if op == OpKind::Stash {
            self.finish_stash_conflict();
            return;
        }
        let message = self.repo.pending_message().unwrap_or_default();
        self.commit = Some(CommitEditor {
            message,
            finish: Some(op),
        });
    }

    fn finish_stash_conflict(&mut self) {
        let remaining = self.repo.conflicted_files().len();
        if remaining > 0 {
            self.refresh_conflict_files();
            self.info(format!("Still {remaining} conflicted file(s)"));
            return;
        }
        self.conflict = None;
        let notice = match self.stash_conflict.take() {
            Some(entry) if entry.pop => self.drop_restored_stash(&entry),
            Some(entry) => format!("Resolved — stash@{{{}}} kept", entry.index),
            None => "Resolved — drop the stash yourself when ready".to_string(),
        };
        self.reload();
        self.refresh_stash();
        self.info(notice);
    }

    fn drop_restored_stash(&mut self, entry: &StashConflict) -> String {
        let listed =
            self.cli.stash_list().into_iter().any(|listed| {
                listed.index == entry.index && listed.message.text() == entry.message
            });
        if !listed {
            return format!("Resolved — stash@{{{}}} kept (list changed)", entry.index);
        }
        match self.cli.stash_drop(entry.index) {
            Ok(()) => format!("Resolved — dropped stash@{{{}}}", entry.index),
            Err(error) => format!("Resolved — {error}"),
        }
    }

    fn finish_conflict_commit(&mut self, op: OpKind, message: &str) {
        if !op.needs_commit() {
            return;
        }
        let previous = self.repo.head_oid();
        match self.cli.commit(message) {
            Ok(()) => {
                if let Some(previous) = previous {
                    self.last_action = Some(match op {
                        OpKind::CherryPick => UndoableAction::CherryPicked { previous },
                        _ => UndoableAction::Merged { previous },
                    });
                }
                self.conflict = None;
                self.reload();
                self.celebrate(match op {
                    OpKind::CherryPick => Event::CherryPick,
                    _ => Event::Merge,
                });
                self.info(format!("Resolved {}", op.label()));
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn request_edit(&mut self) {
        if let Some(path) = self
            .conflict
            .as_ref()
            .and_then(ConflictBrowser::focused_file)
            .map(|file| file.path.clone())
        {
            self.edit_request = Some(path);
        }
    }

    pub fn pending_edit_path(&self) -> Option<std::path::PathBuf> {
        let path = self.edit_request.as_ref()?;
        Some(
            self.repo
                .workdir()
                .unwrap_or_else(|| self.repo.git_dir())
                .join(path),
        )
    }

    pub fn finish_edit(&mut self) {
        let Some(path) = self.edit_request.take() else {
            return;
        };
        if let Some(content) = self.repo.read_workdir_file(&path)
            && conflict_count(&parse(&content)) == 0
        {
            let _ = self.cli.stage_file(&path);
        }
        self.refresh_conflict_files();
    }

    fn abort_conflict(&mut self) {
        let Some(browser) = self.conflict.as_ref() else {
            return;
        };
        let op = browser.op;
        if op == OpKind::Stash {
            self.request_abort_stash_conflict();
            return;
        }
        let result = match op {
            OpKind::CherryPick => self.cli.cherry_pick_abort(),
            OpKind::Rebase => self.cli.rebase_abort(),
            _ => self.cli.merge_abort(),
        };
        self.conflict = None;
        match result {
            Ok(()) => self.info(format!("Aborted {}", op.label())),
            Err(error) => self.fail(error.to_string()),
        }
        self.reload();
    }

    fn request_abort_stash_conflict(&mut self) {
        let paths = self.stash_restore_paths();
        if paths.is_empty() {
            self.conflict = None;
            self.stash_conflict = None;
            self.info("Nothing left to discard".to_string());
            self.reload();
            return;
        }
        let kept = match &self.stash_conflict {
            Some(entry) => format!("stash@{{{}}} is kept", entry.index),
            None => "the stash is kept".to_string(),
        };
        self.confirm = Some(Confirm {
            message: format!("Discard {} file(s)? {kept} (y/n)", paths.len()),
            kind: ConfirmKind::AbortStashConflict,
        });
    }

    fn stash_restore_paths(&self) -> Vec<String> {
        let mut paths = self.repo.conflicted_files();
        if let Some(entry) = &self.stash_conflict {
            for path in self.cli.stash_paths(entry.index) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths.retain(|path| self.repo.known_path(path));
        paths
    }

    fn abort_stash_conflict(&mut self) {
        let paths = self.stash_restore_paths();
        let result = self.cli.restore_from_head(&paths);
        self.conflict = None;
        self.stash_conflict = None;
        match result {
            Ok(()) => self.info("Discarded stash conflicts — stash kept".to_string()),
            Err(error) => self.fail(error.to_string()),
        }
        self.reload();
        self.refresh_stash();
    }

    fn request_delete_branch(&mut self) {
        let Some(entry) = self.branch.as_ref().and_then(BranchPanel::focused) else {
            return;
        };
        let name = entry.name.clone();
        let is_head = entry.is_head;
        let is_remote = entry.remote.is_some();
        let upstream = entry.upstream.clone();

        if is_remote {
            if let Some((remote, remote_branch)) = split_remote_ref(&name) {
                self.confirm = Some(Confirm {
                    message: format!("Delete remote {name}? (y/n)"),
                    kind: ConfirmKind::DeleteRemoteBranch {
                        remote,
                        remote_branch,
                    },
                });
            }
            return;
        }
        if is_head {
            self.alert = Some(format!(
                "'{name}' is the current branch — checkout another first"
            ));
            return;
        }
        let oid = self.repo.branch_tip(&name);
        self.confirm = Some(Confirm {
            message: format!("Delete branch {name}? (y/n)"),
            kind: ConfirmKind::DeleteBranch {
                name,
                oid,
                upstream,
            },
        });
    }

    fn finish_branch_delete(
        &mut self,
        name: String,
        oid: Option<String>,
        upstream: Option<String>,
    ) {
        self.last_action = oid.map(|oid| UndoableAction::DeletedBranch { name, oid });
        self.notice = None;
        self.reload();
        if let Some(upstream) = upstream
            && let Some((remote, remote_branch)) = split_remote_ref(&upstream)
        {
            self.confirm = Some(Confirm {
                message: format!("Delete remote {upstream} too? (y/n)"),
                kind: ConfirmKind::DeleteRemoteBranch {
                    remote,
                    remote_branch,
                },
            });
        }
    }

    fn spawn_delete_remote(&mut self, remote: String, remote_branch: String) {
        self.spawn_remote(
            "deleting remote",
            OnComplete::Notice("Deleted remote branch"),
            move |cli| cli.delete_remote_branch(&remote, &remote_branch),
        );
    }

    fn refresh_branch_entries(&mut self) {
        if self.branch.is_none() {
            return;
        }
        self.branch_all = self.load_branch_entries();
        self.apply_branch_filter();
    }

    fn branch_start(&self) -> String {
        if !self.on_wip
            && let Some(commit) = self.selected_commit()
        {
            return commit.id.to_string();
        }
        self.repo.head_ref().unwrap_or_else(|| "HEAD".to_string())
    }

    fn open_branch_create(&mut self) {
        let start = self.branch_start();
        self.branch_create = Some(BranchCreate {
            name: String::new(),
            start: BranchStart::Commit(start),
        });
    }

    fn branch_name_input(&mut self, character: char) {
        if let Some(editor) = &mut self.branch_create {
            editor.name.push(character);
        }
    }

    fn branch_name_backspace(&mut self) {
        if let Some(editor) = &mut self.branch_create {
            editor.name.pop();
        }
    }

    fn branch_name_submit(&mut self) {
        let Some(editor) = self.branch_create.take() else {
            return;
        };
        let name = editor.name.trim().to_string();
        if name.is_empty() {
            self.info("Empty branch name".to_string());
            self.branch_create = Some(editor);
            return;
        }
        let start = match editor.start {
            BranchStart::Stash(index) => {
                self.stash_branch(index, &name);
                return;
            }
            BranchStart::Commit(ref start) => start.clone(),
        };
        let previous = self.repo.head_ref();
        match self.cli.create_branch(&name, &start) {
            Ok(()) => {
                if let Some(previous) = previous {
                    self.last_action = Some(UndoableAction::CreatedBranch { name, previous });
                }
                self.notice = None;
                self.close_branch_panel();
                self.reload();
            }
            Err(error) => {
                self.fail(error.to_string());
                self.branch_create = Some(editor);
            }
        }
    }

    pub fn report_watch_degraded(&mut self) {
        self.fail("Watching the working tree failed; refreshes may lag".to_string());
    }

    pub fn poll_status(&mut self) {
        if self.repo.state_stamp() != self.state_stamp {
            self.reload();
            return;
        }
        if self.status_retries > 0 {
            self.refresh_status();
        }
    }

    fn refresh_status(&mut self) {
        let stamp = self.repo.index_stamp();
        let moved = stamp != self.index_stamp;
        if moved {
            self.repo.reread_index();
        }
        if moved || self.status_retries == 0 {
            self.status_retries = STATUS_RETRIES;
        }
        match self.repo.working_status() {
            Ok(status) => {
                self.status = status;
                self.index_stamp = stamp;
                self.status_retries = 0;
            }
            Err(error) => {
                self.status_retries -= 1;
                if self.status_retries == 0 {
                    self.fail(format!("Could not read the working tree: {error}"));
                }
            }
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
        self.state_stamp = self.repo.state_stamp();
        self.graph_dims = None;
        let selected_id = self.commits.get(self.selected).map(|commit| commit.id);
        if let Ok(meta) = self.repo.meta() {
            self.meta = meta;
        }
        if let Ok(oids) = self.repo.commit_oids(&self.visibility) {
            let keep = self.commits.len().max(self.load_page).min(oids.len());
            self.commit_oids = oids;
            self.commits = self.repo.hydrate(&self.commit_oids[..keep]);
            self.exhausted = self.commits.len() >= self.commit_oids.len();
            self.rebuild_rows();
        }

        self.selected = selected_id
            .and_then(|id| self.find_loading(id))
            .or_else(|| self.head_row())
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
        self.refresh_branch_entries();
        self.refresh_submodule_panel();
        self.refresh_parent_sync();
        self.detect_conflict();
    }

    fn head_row(&self) -> Option<usize> {
        let head = self.repo.head_oid()?;
        self.commits
            .iter()
            .position(|commit| commit.id.to_string() == head)
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
        let start = self.commits.len();
        let end = (start + self.load_page).min(self.commit_oids.len());
        if start >= end {
            self.exhausted = true;
            return;
        }
        let mut more = self.repo.hydrate(&self.commit_oids[start..end]);
        if more.is_empty() {
            self.exhausted = true;
            return;
        }
        let new_rows = self.graph_layout.extend(&graph_commits(&more));
        self.commits.append(&mut more);
        self.rows.extend(new_rows);
        self.graph_dims = None;
        if self.commits.len() >= self.commit_oids.len() {
            self.exhausted = true;
        }
    }

    fn load_all(&mut self) {
        while !self.exhausted {
            self.load_more();
        }
    }

    fn rebuild_rows(&mut self) {
        self.graph_layout = GraphLayout::new();
        self.rows = self.graph_layout.extend(&graph_commits(&self.commits));
    }

    fn tick(&mut self, dt: Duration) {
        if let Some(remaining) = &mut self.peek_debounce {
            *remaining -= dt.as_secs_f32();
            if *remaining <= 0.0 {
                self.peek_debounce = None;
                self.fill_peek();
            }
        }
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
        if slide::advance_panel(&mut self.branch, step) {
            self.branch_filter = None;
        }
        if let Some(menu) = &mut self.join {
            menu.panel.advance(step);
            if menu.panel.is_dismissed() {
                self.join = None;
            }
        }
        if let Some(view) = &mut self.stash {
            view.panel.advance(step);
            if view.panel.is_dismissed() {
                self.stash = None;
            }
        }
        if slide::advance_panel(&mut self.focus_panel, step) {
            self.focus_name = None;
        }
        slide::advance_panel(&mut self.submodule_panel, step);
        if let Some(celebration) = &mut self.celebration {
            celebration.progress -= dt.as_secs_f32() / CELEBRATION_DURATION.as_secs_f32();
            if celebration.progress <= 0.0 {
                self.celebration = None;
            }
        }
        if let Some(job) = &mut self.remote {
            job.spinner = job.spinner.wrapping_add(1);
        }
        if let Some(transition) = &mut self.nav_transition {
            transition.progress += dt.as_secs_f32() / NAV_TRANSITION.as_secs_f32();
            if transition.progress >= 1.0 {
                self.nav_transition = None;
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

fn short_oid(oid: &str) -> String {
    oid.chars().take(7).collect()
}

fn discard_mode(file: &WorkingFile) -> DiscardMode {
    match file.state {
        StageState::Untracked => DiscardMode::Untracked,
        StageState::Staged => DiscardMode::Head,
        StageState::Unstaged if file.status == CONFLICTED => DiscardMode::Head,
        StageState::Unstaged => DiscardMode::Worktree,
    }
}

fn split_remote_ref(reference: &str) -> Option<(String, String)> {
    reference
        .split_once('/')
        .map(|(remote, branch)| (remote.to_string(), branch.to_string()))
}

fn merge_note(prediction: &MergePrediction) -> String {
    match prediction {
        MergePrediction::UpToDate => "up to date".to_string(),
        MergePrediction::FastForward => "fast-forward".to_string(),
        MergePrediction::Clean => "clean".to_string(),
        MergePrediction::Conflicts { files } => format!("{} conflicts", files.len()),
    }
}

fn fast_forward_note(prediction: &MergePrediction) -> String {
    match prediction {
        MergePrediction::FastForward => "fast-forward".to_string(),
        MergePrediction::UpToDate => "up to date".to_string(),
        _ => "not possible (diverged)".to_string(),
    }
}

fn rebase_note(prediction: &MergePrediction, ahead: usize, attached: bool) -> String {
    if !attached {
        return "detached HEAD".to_string();
    }
    if matches!(prediction, MergePrediction::UpToDate) {
        return "up to date".to_string();
    }
    let plural = if ahead == 1 { "" } else { "s" };
    format!("replays {ahead} commit{plural}")
}

fn cherry_pick_note(prediction: &MergePrediction) -> String {
    match prediction {
        MergePrediction::Clean | MergePrediction::FastForward => "clean".to_string(),
        MergePrediction::UpToDate => "already applied".to_string(),
        MergePrediction::Conflicts { files } => format!("{} conflicts", files.len()),
    }
}

fn join_summary(prediction: &MergePrediction, is_branch: bool, incoming: usize) -> String {
    match prediction {
        MergePrediction::UpToDate => "already up to date".to_string(),
        MergePrediction::FastForward => format!("+{incoming} commits · fast-forward · linear"),
        MergePrediction::Clean if is_branch => {
            format!("+{incoming} commits · merge commit · 2 parents · clean")
        }
        MergePrediction::Clean => "cherry-pick · clean".to_string(),
        MergePrediction::Conflicts { files } => {
            format!("{} conflicting file(s) · resolve in 3.3", files.len())
        }
    }
}

fn graph_commits(commits: &[CommitInfo]) -> Vec<GraphCommit<Oid>> {
    commits
        .iter()
        .map(|commit| GraphCommit {
            id: commit.id,
            parents: commit.parents.clone(),
        })
        .collect()
}
