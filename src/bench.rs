use std::collections::HashSet;
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::mpsc;

use git2::Oid;

use crate::git::{CommitInfo, Repo};
use crate::mutate::GitCli;
use crate::review::{ReviewRow, detect_base, own_commits};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Pickable,
    Merge,
    AlreadyInDestination,
}

pub fn classify<Id: Clone + Eq + Hash>(
    rows: &[ReviewRow<Id>],
    missing_from_destination: &HashSet<Id>,
) -> Vec<(Id, RowKind)> {
    rows.iter()
        .map(|row| match row {
            ReviewRow::Merge(id) => (id.clone(), RowKind::Merge),
            ReviewRow::Commit(id) if missing_from_destination.contains(id) => {
                (id.clone(), RowKind::Pickable)
            }
            ReviewRow::Commit(id) => (id.clone(), RowKind::AlreadyInDestination),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickRef<Id> {
    pub id: Id,
    pub branch: String,
    pub depth: usize,
    pub time: i64,
}

pub fn application_order<Id: Clone>(picks: &[PickRef<Id>]) -> Vec<Id> {
    let mut queues: Vec<Vec<&PickRef<Id>>> = Vec::new();
    for pick in picks {
        match queues
            .iter_mut()
            .find(|queue| queue[0].branch == pick.branch)
        {
            Some(queue) => queue.push(pick),
            None => queues.push(vec![pick]),
        }
    }
    for queue in &mut queues {
        queue.sort_by_key(|pick| pick.depth);
    }
    let mut order = Vec::with_capacity(picks.len());
    while let Some(next) = queues
        .iter_mut()
        .filter(|queue| !queue.is_empty())
        .min_by(|a, b| {
            let (a, b) = (a.last().unwrap(), b.last().unwrap());
            a.time.cmp(&b.time).then_with(|| a.branch.cmp(&b.branch))
        })
    {
        order.push(next.pop().unwrap().id.clone());
    }
    order
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prediction {
    Clean,
    Conflicts(Vec<String>),
    Failed,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    Conflicts(Vec<String>),
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replay<Tip> {
    pub predictions: Vec<Prediction>,
    pub tip: Option<Tip>,
}

pub fn chain<Id, Tip>(
    start: Tip,
    picks: &[Id],
    mut replay: impl FnMut(&Tip, &Id) -> Result<Tip, Blocked>,
) -> Replay<Tip> {
    let mut predictions = Vec::with_capacity(picks.len());
    let mut tip = Some(start);
    for pick in picks {
        let Some(onto) = &tip else {
            predictions.push(Prediction::Pending);
            continue;
        };
        match replay(onto, pick) {
            Ok(next) => {
                predictions.push(Prediction::Clean);
                tip = Some(next);
            }
            Err(blocked) => {
                predictions.push(match blocked {
                    Blocked::Conflicts(files) => Prediction::Conflicts(files),
                    Blocked::Failed => Prediction::Failed,
                });
                tip = None;
            }
        }
    }
    Replay { predictions, tip }
}

const DESTINATION_LOG: usize = 60;
const FLIGHT: f32 = 0.15;
const HOLD: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arrival {
    Flying(f32),
    Landed,
}

pub struct Landing {
    ids: Vec<Oid>,
    elapsed: f32,
}

impl Landing {
    pub fn arrival(&self, id: Oid) -> Option<Arrival> {
        let order = self.ids.iter().position(|landed| *landed == id)?;
        let progress = (self.elapsed - order as f32 * FLIGHT) / FLIGHT;
        Some(if progress < 1.0 {
            Arrival::Flying(progress.max(0.0))
        } else {
            Arrival::Landed
        })
    }

    fn is_over(&self) -> bool {
        self.elapsed > self.ids.len() as f32 * FLIGHT + HOLD
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchFocus {
    Source,
    Destination,
}

pub struct BenchRow {
    pub commit: CommitInfo,
    pub kind: RowKind,
    pub depth: usize,
}

pub struct Pick {
    pub commit: CommitInfo,
    pub branch: String,
    pub depth: usize,
}

pub struct CherryPickBench {
    pub source: String,
    pub destination: String,
    pub rows: Vec<BenchRow>,
    pub selected: usize,
    pub picks: Vec<Pick>,
    pub focus: BenchFocus,
    pub destination_log: Vec<CommitInfo>,
    pub source_scroll: u16,
    pub destination_scroll: u16,
    pub unfold: f32,
    unfold_target: f32,
    workdir: PathBuf,
    analysis_rx: Option<mpsc::Receiver<Analysis>>,
    pub dropped: usize,
    pub predictions: Vec<(Oid, Prediction)>,
    pub landing: Option<Landing>,
    entry_pick: Option<Oid>,
}

struct Analysis {
    missing: HashSet<Oid>,
    predictions: Vec<(Oid, Prediction)>,
}

impl CherryPickBench {
    pub fn open(repo: &Repo, source: &str, destination: &str) -> Option<Self> {
        let mut bench = Self {
            source: source.to_string(),
            destination: destination.to_string(),
            rows: Vec::new(),
            selected: 0,
            picks: Vec::new(),
            focus: BenchFocus::Source,
            destination_log: Vec::new(),
            source_scroll: 0,
            destination_scroll: 0,
            unfold: 0.0,
            unfold_target: 1.0,
            workdir: repo
                .workdir()
                .unwrap_or_else(|| repo.git_dir())
                .to_path_buf(),
            analysis_rx: None,
            dropped: 0,
            predictions: Vec::new(),
            landing: None,
            entry_pick: None,
        };
        bench.reload(repo)?;
        Some(bench)
    }

    pub fn reload(&mut self, repo: &Repo) -> Option<()> {
        let tip = repo.branch_oid(&self.source)?;
        let destination = repo.branch_oid(&self.destination)?;
        let base = detect_base(repo, &self.source)
            .and_then(|name| repo.branch_oid(&name))
            .unwrap_or(destination);
        let ids = own_commits(tip, base, |id| repo.parent_ids(*id));
        let oids: Vec<Oid> = ids.iter().map(|row| *row.id()).collect();
        let unknown: HashSet<Oid> = oids.iter().copied().collect();
        let kinds = classify(&ids, &unknown);
        self.rows = repo
            .hydrate(&oids)
            .into_iter()
            .zip(kinds)
            .enumerate()
            .map(|(depth, (commit, (_, kind)))| BenchRow {
                commit,
                kind,
                depth,
            })
            .collect();
        self.destination_log = repo.hydrate(&first_parent_line(repo, destination, DESTINATION_LOG));
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.analyze();
        Some(())
    }

    fn analyze(&mut self) {
        let destination = self.destination.clone();
        let picks: Vec<(Oid, Option<Oid>)> = self
            .ordered_picks()
            .into_iter()
            .map(|pick| (pick.commit.id, pick.commit.parents.first().copied()))
            .collect();
        let mut sources: Vec<String> = vec![self.source.clone()];
        for pick in &self.picks {
            if !sources.contains(&pick.branch) {
                sources.push(pick.branch.clone());
            }
        }
        let cli = GitCli::new(&self.workdir);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let missing = sources
                .iter()
                .flat_map(|source| cli.missing_from(&destination, source))
                .filter_map(|id| Oid::from_str(&id).ok())
                .collect();
            let ids: Vec<Oid> = picks.iter().map(|(id, _)| *id).collect();
            let replay = chain(destination, &picks, |onto, (pick, parent)| {
                cli.replay_tree(
                    &pick.to_string(),
                    parent.map(|parent| parent.to_string()).as_deref(),
                    onto,
                )
            });
            let predictions = ids.into_iter().zip(replay.predictions).collect();
            let _ = tx.send(Analysis {
                missing,
                predictions,
            });
        });
        self.analysis_rx = Some(rx);
    }

    pub fn analysis_pending(&self) -> bool {
        self.analysis_rx.is_some()
    }

    pub fn poll_background(&mut self) {
        let Some(rx) = &self.analysis_rx else {
            return;
        };
        let analysis = match rx.try_recv() {
            Ok(analysis) => analysis,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.analysis_rx = None;
                return;
            }
            Err(mpsc::TryRecvError::Empty) => return,
        };
        self.analysis_rx = None;
        self.predictions = analysis.predictions;
        for row in &mut self.rows {
            if row.kind != RowKind::Merge {
                row.kind = if analysis.missing.contains(&row.commit.id) {
                    RowKind::Pickable
                } else {
                    RowKind::AlreadyInDestination
                };
            }
        }
        let before = self.picks.len();
        self.picks
            .retain(|pick| analysis.missing.contains(&pick.commit.id));
        self.dropped = before - self.picks.len();
        if let Some(id) = self.entry_pick.take()
            && self.rows.iter().any(|row| row.commit.id == id)
        {
            self.select(id);
            self.toggle_pick();
        }
    }

    pub fn pick_once_classified(&mut self, id: Oid) {
        self.select(id);
        self.entry_pick = Some(id);
    }

    pub fn select(&mut self, id: Oid) {
        if let Some(index) = self.rows.iter().position(|row| row.commit.id == id) {
            self.selected = index;
        }
    }

    pub fn toggle_pick(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        if row.kind != RowKind::Pickable {
            return;
        }
        let id = row.commit.id;
        if let Some(index) = self.picks.iter().position(|pick| pick.commit.id == id) {
            self.picks.remove(index);
        } else {
            self.picks.push(Pick {
                commit: row.commit.clone(),
                branch: self.source.clone(),
                depth: row.depth,
            });
        }
        self.analyze();
    }

    pub fn prediction(&self, id: Oid) -> Option<&Prediction> {
        self.predictions
            .iter()
            .find(|(pick, _)| *pick == id)
            .map(|(_, prediction)| prediction)
    }

    pub fn prediction_failed(&self) -> bool {
        self.predictions
            .iter()
            .any(|(_, prediction)| *prediction == Prediction::Failed)
    }

    pub fn predicted_conflict(&self) -> bool {
        self.predictions
            .iter()
            .any(|(_, prediction)| matches!(prediction, Prediction::Conflicts(_)))
    }

    pub fn copy_onto_destination(&self, repo: &Repo, cli: &GitCli) -> Option<(String, String)> {
        let previous = repo.branch_tip(&self.destination)?;
        let picks = self.ordered_picks();
        let replay = chain(previous.clone(), &picks, |onto, pick| {
            let parent = pick.commit.parents.first().map(Oid::to_string);
            let tree = cli.replay_tree(&pick.commit.id.to_string(), parent.as_deref(), onto)?;
            let authorship = repo.authorship(pick.commit.id).ok_or(Blocked::Failed)?;
            cli.commit_tree(&tree, onto, &authorship)
                .map_err(|_| Blocked::Failed)
        });
        let written = replay.tip?;
        cli.update_branch(&self.destination, &written, &previous)
            .ok()?;
        Some((previous, written))
    }

    pub fn clear_picks(&mut self) {
        self.picks.clear();
        self.predictions.clear();
    }

    pub fn is_picked(&self, id: Oid) -> bool {
        self.picks.iter().any(|pick| pick.commit.id == id)
    }

    pub fn ordered_picks(&self) -> Vec<&Pick> {
        let refs: Vec<PickRef<usize>> = self
            .picks
            .iter()
            .enumerate()
            .map(|(index, pick)| PickRef {
                id: index,
                branch: pick.branch.clone(),
                depth: pick.depth,
                time: pick.commit.time,
            })
            .collect();
        application_order(&refs)
            .into_iter()
            .map(|index| &self.picks[index])
            .collect()
    }

    pub fn branch_count(&self) -> usize {
        let branches: HashSet<&str> = self.picks.iter().map(|pick| pick.branch.as_str()).collect();
        branches.len()
    }

    pub fn retarget(&mut self, repo: &Repo, source: &str, destination: &str) {
        if let Some(id) = self.entry_pick.take() {
            self.select(id);
            self.toggle_pick();
        }
        let previous = (
            std::mem::replace(&mut self.source, source.to_string()),
            std::mem::replace(&mut self.destination, destination.to_string()),
        );
        self.selected = 0;
        self.source_scroll = 0;
        self.destination_scroll = 0;
        if self.reload(repo).is_none() {
            (self.source, self.destination) = previous;
            self.reload(repo);
        }
    }

    pub fn draft(&self) -> BenchDraft {
        BenchDraft {
            source: Some(self.source.clone()),
            destination: Some(self.destination.clone()),
            pick: None,
        }
    }

    pub fn move_down(&mut self, delta: isize) {
        match self.focus {
            BenchFocus::Source => {
                self.selected = crate::slide::clamp_index(self.selected, delta, self.rows.len());
            }
            BenchFocus::Destination => {
                self.destination_scroll = (self.destination_scroll as isize + delta).max(0) as u16;
            }
        }
    }

    pub fn rotate_focus(&mut self) {
        self.focus = match self.focus {
            BenchFocus::Source => BenchFocus::Destination,
            BenchFocus::Destination => BenchFocus::Source,
        };
    }

    pub fn pick_all(&mut self) {
        let fresh: Vec<Pick> = self
            .rows
            .iter()
            .filter(|row| row.kind == RowKind::Pickable && !self.is_picked(row.commit.id))
            .map(|row| Pick {
                commit: row.commit.clone(),
                branch: self.source.clone(),
                depth: row.depth,
            })
            .collect();
        self.picks.extend(fresh);
        self.analyze();
    }

    pub fn is_animating(&self) -> bool {
        self.is_folding() || self.analysis_pending() || self.landing.is_some()
    }

    pub fn land(&mut self, count: usize) {
        let ids = self
            .destination_log
            .iter()
            .take(count)
            .rev()
            .map(|commit| commit.id)
            .collect();
        self.landing = Some(Landing { ids, elapsed: 0.0 });
    }

    pub fn advance_landing(&mut self, seconds: f32) {
        if let Some(landing) = &mut self.landing {
            landing.elapsed += seconds;
            if landing.is_over() {
                self.landing = None;
            }
        }
    }

    pub fn fold(&mut self) {
        self.unfold_target = 0.0;
    }

    pub fn unfold_instantly(&mut self) {
        self.unfold = self.unfold_target;
    }

    pub fn is_folding(&self) -> bool {
        self.unfold != self.unfold_target
    }

    pub fn advance_unfold(&mut self, step: f32) -> bool {
        crate::slide::advance(&mut self.unfold, self.unfold_target, step);
        self.unfold_target == 0.0 && self.unfold <= 0.0
    }
}

fn first_parent_line(repo: &Repo, tip: Oid, limit: usize) -> Vec<Oid> {
    let mut line = Vec::with_capacity(limit);
    let mut cursor = Some(tip);
    while let Some(id) = cursor {
        if line.len() == limit {
            break;
        }
        line.push(id);
        cursor = repo.parent_ids(id).first().copied();
    }
    line
}

pub fn source_candidates(repo: &Repo, commit: Oid, containing: &[String]) -> Vec<String> {
    let id = commit.to_string();
    let on_trunk =
        crate::review::base_candidates("", None, repo.remote_default().as_deref(), |name| {
            repo.branch_exists(name)
        })
        .iter()
        .filter_map(|name| repo.branch_tip(name))
        .any(|tip| repo.is_ancestor(&id, &tip));
    if on_trunk {
        return Vec::new();
    }
    let owns = |branch: &String| {
        let Some(tip) = repo.branch_oid(branch) else {
            return false;
        };
        let Some(base) = detect_base(repo, branch).and_then(|name| repo.branch_oid(&name)) else {
            return false;
        };
        own_commits(tip, base, |id| repo.parent_ids(*id))
            .iter()
            .any(|row| row.commit() == Some(&commit))
    };
    let (local, remote): (Vec<&String>, Vec<&String>) = containing
        .iter()
        .partition(|name| repo.has_local_branch(name));
    let owners: Vec<String> = local
        .into_iter()
        .filter(|name| owns(name))
        .cloned()
        .collect();
    if !owners.is_empty() {
        return owners;
    }
    remote
        .into_iter()
        .filter(|name| owns(name))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchEnd {
    Source,
    Destination,
}

#[derive(Debug, Clone, Default)]
pub struct BenchDraft {
    pub source: Option<String>,
    pub destination: Option<String>,
    pub pick: Option<Oid>,
}

pub struct BenchPicker {
    pub end: BenchEnd,
    pub query: String,
    pub draft: BenchDraft,
    names: Vec<String>,
    matches: Vec<usize>,
    pub selected: usize,
}

impl BenchPicker {
    pub fn opening(end: BenchEnd, names: Vec<String>, draft: BenchDraft) -> Self {
        Self {
            end,
            query: String::new(),
            draft,
            matches: (0..names.len()).collect(),
            names,
            selected: 0,
        }
    }

    pub fn input(&mut self, character: char) {
        self.query.push(character);
        self.refilter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refilter();
    }

    fn refilter(&mut self) {
        let names: Vec<&str> = self.names.iter().map(String::as_str).collect();
        self.matches = crate::palette::fuzzy_filter(&self.query, &names);
        self.selected = 0;
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = crate::slide::clamp_index(self.selected, delta, self.matches.len());
    }

    pub fn rows(&self) -> Vec<&str> {
        self.matches
            .iter()
            .map(|&index| self.names[index].as_str())
            .collect()
    }

    pub fn chosen(&self) -> Option<&str> {
        let index = *self.matches.get(self.selected)?;
        Some(self.names[index].as_str())
    }
}
