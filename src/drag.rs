#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRef {
    pub oid: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropIntent {
    pub source_branch: String,
    pub target_oid: String,
    pub target_branch: Option<String>,
}

pub fn resolve_drop(rows: &[RowRef], down: usize, up: usize) -> Option<DropIntent> {
    if down == up {
        return None;
    }
    let source = rows.get(down)?;
    let target = rows.get(up)?;
    let source_branch = source.branch.clone()?;
    if source.oid == target.oid {
        return None;
    }
    Some(DropIntent {
        source_branch,
        target_oid: target.oid.clone(),
        target_branch: target.branch.clone(),
    })
}
