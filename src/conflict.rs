#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Context(Vec<String>),
    Conflict {
        ours: Vec<String>,
        theirs: Vec<String>,
    },
}

enum Mode {
    Context,
    Ours,
    Base,
    Theirs,
}

pub fn parse(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut context = Vec::new();
    let mut ours = Vec::new();
    let mut theirs = Vec::new();
    let mut mode = Mode::Context;

    for line in text.lines() {
        if line.starts_with("<<<<<<<") {
            if !context.is_empty() {
                segments.push(Segment::Context(std::mem::take(&mut context)));
            }
            mode = Mode::Ours;
        } else if line.starts_with("|||||||") && !matches!(mode, Mode::Context) {
            mode = Mode::Base;
        } else if line.starts_with("=======") && !matches!(mode, Mode::Context | Mode::Theirs) {
            mode = Mode::Theirs;
        } else if line.starts_with(">>>>>>>") && matches!(mode, Mode::Theirs) {
            segments.push(Segment::Conflict {
                ours: std::mem::take(&mut ours),
                theirs: std::mem::take(&mut theirs),
            });
            mode = Mode::Context;
        } else {
            match mode {
                Mode::Context => context.push(line.to_string()),
                Mode::Ours => ours.push(line.to_string()),
                Mode::Base => {}
                Mode::Theirs => theirs.push(line.to_string()),
            }
        }
    }

    if !context.is_empty() {
        segments.push(Segment::Context(context));
    }
    segments
}

pub fn resolve(segments: &[Segment], choices: &[Side]) -> String {
    let mut lines = Vec::new();
    let mut index = 0;
    for segment in segments {
        match segment {
            Segment::Context(context) => lines.extend(context.iter().cloned()),
            Segment::Conflict { ours, theirs } => {
                let side = choices.get(index).copied().unwrap_or(Side::Ours);
                let chosen = match side {
                    Side::Ours => ours,
                    Side::Theirs => theirs,
                };
                lines.extend(chosen.iter().cloned());
                index += 1;
            }
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

pub fn conflict_count(segments: &[Segment]) -> usize {
    segments
        .iter()
        .filter(|segment| matches!(segment, Segment::Conflict { .. }))
        .count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Merge,
    CherryPick,
    Rebase,
}

impl OpKind {
    pub fn label(self) -> &'static str {
        match self {
            OpKind::Merge => "merge",
            OpKind::CherryPick => "cherry-pick",
            OpKind::Rebase => "rebase",
        }
    }
}

pub struct ConflictFile {
    pub path: String,
    pub segments: Vec<Segment>,
    pub choices: Vec<Option<Side>>,
}

impl ConflictFile {
    pub fn new(path: String, text: &str) -> Self {
        let segments = parse(text);
        let count = conflict_count(&segments);
        Self {
            path,
            segments,
            choices: vec![None; count],
        }
    }

    pub fn count(&self) -> usize {
        self.choices.len()
    }

    pub fn is_resolved(&self) -> bool {
        self.choices.iter().all(Option::is_some)
    }

    pub fn resolved_text(&self) -> Option<String> {
        if !self.is_resolved() {
            return None;
        }
        let choices: Vec<Side> = self.choices.iter().flatten().copied().collect();
        Some(resolve(&self.segments, &choices))
    }
}

pub struct ConflictBrowser {
    pub op: OpKind,
    pub files: Vec<ConflictFile>,
    pub file: usize,
    pub block: usize,
    pub progress: Option<(usize, usize)>,
}

impl ConflictBrowser {
    pub fn new(op: OpKind, files: Vec<ConflictFile>, progress: Option<(usize, usize)>) -> Self {
        Self {
            op,
            files,
            file: 0,
            block: 0,
            progress,
        }
    }

    pub fn focused_file(&self) -> Option<&ConflictFile> {
        self.files.get(self.file)
    }

    pub fn move_file(&mut self, delta: isize) {
        if self.files.is_empty() {
            return;
        }
        let last = (self.files.len() - 1) as isize;
        self.file = (self.file as isize + delta).clamp(0, last) as usize;
        self.block = 0;
    }

    pub fn move_block(&mut self, delta: isize) {
        let count = self.focused_file().map_or(0, ConflictFile::count);
        if count == 0 {
            self.block = 0;
            return;
        }
        let last = (count - 1) as isize;
        self.block = (self.block as isize + delta).clamp(0, last) as usize;
    }

    pub fn remaining(&self) -> usize {
        self.files
            .iter()
            .flat_map(|file| file.choices.iter())
            .filter(|choice| choice.is_none())
            .count()
    }
}
