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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitRow {
    pub left: Option<String>,
    pub right: Option<String>,
}

pub fn split_rows(lines: &[String]) -> Vec<SplitRow> {
    let mut rows = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        match marker_of(&lines[index]) {
            '-' => {
                let removed_start = index;
                while index < lines.len() && marker_of(&lines[index]) == '-' {
                    index += 1;
                }
                let removed = &lines[removed_start..index];
                let added_start = index;
                while index < lines.len() && marker_of(&lines[index]) == '+' {
                    index += 1;
                }
                let added = &lines[added_start..index];
                for offset in 0..removed.len().max(added.len()) {
                    rows.push(SplitRow {
                        left: removed.get(offset).cloned(),
                        right: added.get(offset).cloned(),
                    });
                }
            }
            '+' => {
                rows.push(SplitRow {
                    left: None,
                    right: Some(lines[index].clone()),
                });
                index += 1;
            }
            _ => {
                rows.push(SplitRow {
                    left: Some(lines[index].clone()),
                    right: Some(lines[index].clone()),
                });
                index += 1;
            }
        }
    }
    rows
}

pub fn marker_of(line: &str) -> char {
    line.chars().next().unwrap_or(' ')
}

pub fn content_of(line: &str) -> &str {
    let marker = marker_of(line);
    &line[marker.len_utf8().min(line.len())..]
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
