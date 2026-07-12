#[derive(Debug, Clone)]
pub struct GraphCommit<Id> {
    pub id: Id,
    pub parents: Vec<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow<Id> {
    pub node_column: usize,
    pub glyphs: String,
    pub node_key: Id,
    pub lane_keys: Vec<Option<Id>>,
}

const NODE: char = '●';
const VERTICAL: char = '│';
const HORIZONTAL: char = '─';
const OPEN_RIGHT: char = '╮';
const CLOSE_RIGHT: char = '╯';
const CROSS: char = '┼';

#[derive(Clone)]
struct Lane<Id> {
    expected: Id,
    key: Id,
}

pub struct GraphLayout<Id> {
    lanes: Vec<Option<Lane<Id>>>,
}

impl<Id: Clone + PartialEq> Default for GraphLayout<Id> {
    fn default() -> Self {
        Self { lanes: Vec::new() }
    }
}

impl<Id: Clone + PartialEq> GraphLayout<Id> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, commits: &[GraphCommit<Id>]) -> Vec<GraphRow<Id>> {
        lay_out_into(&mut self.lanes, commits)
    }
}

pub fn lay_out<Id: Clone + PartialEq>(commits: &[GraphCommit<Id>]) -> Vec<GraphRow<Id>> {
    let mut lanes: Vec<Option<Lane<Id>>> = Vec::new();
    lay_out_into(&mut lanes, commits)
}

fn lay_out_into<Id: Clone + PartialEq>(
    lanes: &mut Vec<Option<Lane<Id>>>,
    commits: &[GraphCommit<Id>],
) -> Vec<GraphRow<Id>> {
    let mut rows = Vec::with_capacity(commits.len());

    for commit in commits {
        let merging: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, lane)| lane.as_ref().map(|lane| &lane.expected) == Some(&commit.id))
            .map(|(index, _)| index)
            .collect();

        let node_column = match merging.first() {
            Some(&first) => first,
            None => free_lane(lanes),
        };

        let node_key = lanes[node_column]
            .as_ref()
            .map(|lane| lane.key.clone())
            .unwrap_or_else(|| commit.id.clone());

        let incoming: Vec<bool> = lanes.iter().map(Option::is_some).collect();
        let incoming_keys: Vec<Option<Id>> = lanes
            .iter()
            .map(|lane| lane.as_ref().map(|lane| lane.key.clone()))
            .collect();

        for &index in &merging {
            lanes[index] = None;
        }

        let mut parents = commit.parents.iter();
        lanes[node_column] = parents.next().map(|parent| Lane {
            expected: parent.clone(),
            key: node_key.clone(),
        });

        let mut opened = Vec::new();
        for parent in parents {
            let slot = free_lane(lanes);
            lanes[slot] = Some(Lane {
                expected: parent.clone(),
                key: parent.clone(),
            });
            opened.push(slot);
        }

        let closed: Vec<usize> = merging
            .iter()
            .copied()
            .filter(|&index| index != node_column)
            .collect();

        let outgoing: Vec<bool> = lanes.iter().map(Option::is_some).collect();
        let lane_keys = (0..lanes.len().max(incoming_keys.len()))
            .map(|index| {
                lanes
                    .get(index)
                    .and_then(|lane| lane.as_ref().map(|lane| lane.key.clone()))
                    .or_else(|| incoming_keys.get(index).cloned().flatten())
            })
            .collect();

        rows.push(GraphRow {
            node_column,
            glyphs: render_row(node_column, &incoming, &outgoing, &opened, &closed),
            node_key,
            lane_keys,
        });
    }

    rows
}

fn free_lane<Id>(lanes: &mut Vec<Option<Lane<Id>>>) -> usize {
    match lanes.iter().position(|lane| lane.is_none()) {
        Some(index) => index,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

fn render_row(
    node_column: usize,
    incoming: &[bool],
    outgoing: &[bool],
    opened: &[usize],
    closed: &[usize],
) -> String {
    let lane_count = incoming.len().max(outgoing.len());
    let mut cells = vec![' '; lane_count * 2];

    for lane in 0..lane_count {
        let present_before = incoming.get(lane).copied().unwrap_or(false);
        let present_after = outgoing.get(lane).copied().unwrap_or(false);
        if lane != node_column && present_before && present_after {
            cells[lane * 2] = VERTICAL;
        }
    }

    for &lane in opened.iter().chain(closed) {
        let glyph = if opened.contains(&lane) {
            OPEN_RIGHT
        } else {
            CLOSE_RIGHT
        };
        for cell in cells.iter_mut().take(lane * 2).skip(node_column * 2 + 1) {
            *cell = if *cell == VERTICAL { CROSS } else { HORIZONTAL };
        }
        cells[lane * 2] = glyph;
    }

    cells[node_column * 2] = NODE;

    let rendered: String = cells.into_iter().collect();
    rendered.trim_end().to_string()
}
