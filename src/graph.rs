#[derive(Debug, Clone)]
pub struct GraphCommit<Id> {
    pub id: Id,
    pub parents: Vec<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    pub node_column: usize,
    pub glyphs: String,
}

const NODE: char = '●';
const VERTICAL: char = '│';
const HORIZONTAL: char = '─';
const OPEN_RIGHT: char = '╮';
const CLOSE_RIGHT: char = '╯';
const CROSS: char = '┼';

pub fn lay_out<Id: Clone + PartialEq>(commits: &[GraphCommit<Id>]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<Id>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());

    for commit in commits {
        let merging: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, lane)| lane.as_ref() == Some(&commit.id))
            .map(|(index, _)| index)
            .collect();

        let node_column = match merging.first() {
            Some(&first) => first,
            None => free_lane(&mut lanes),
        };

        let incoming = lanes.clone();

        for &index in &merging {
            lanes[index] = None;
        }

        let mut parents = commit.parents.iter();
        lanes[node_column] = parents.next().cloned();

        let mut opened = Vec::new();
        for parent in parents {
            let slot = free_lane(&mut lanes);
            lanes[slot] = Some(parent.clone());
            opened.push(slot);
        }

        let closed: Vec<usize> = merging
            .iter()
            .copied()
            .filter(|&index| index != node_column)
            .collect();

        rows.push(GraphRow {
            node_column,
            glyphs: render_row(node_column, &incoming, &lanes, &opened, &closed),
        });
    }

    rows
}

fn free_lane<Id>(lanes: &mut Vec<Option<Id>>) -> usize {
    match lanes.iter().position(|lane| lane.is_none()) {
        Some(index) => index,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

fn render_row<Id>(
    node_column: usize,
    incoming: &[Option<Id>],
    outgoing: &[Option<Id>],
    opened: &[usize],
    closed: &[usize],
) -> String {
    let lane_count = incoming.len().max(outgoing.len());
    let mut cells = vec![' '; lane_count * 2];

    for lane in 0..lane_count {
        let present_before = incoming.get(lane).is_some_and(Option::is_some);
        let present_after = outgoing.get(lane).is_some_and(Option::is_some);
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
