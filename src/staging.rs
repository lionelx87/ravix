#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
}

pub fn build_patch(diff: &FileDiff, selected: &[usize]) -> String {
    let mut patch = format!(
        "diff --git a/{old} b/{new}\n--- a/{old}\n+++ b/{new}\n",
        old = diff.old_path,
        new = diff.new_path,
    );

    for &index in selected {
        let Some(hunk) = diff.hunks.get(index) else {
            continue;
        };
        patch.push_str(&hunk.header);
        patch.push('\n');
        for line in &hunk.lines {
            patch.push_str(line);
            patch.push('\n');
        }
    }

    patch
}
