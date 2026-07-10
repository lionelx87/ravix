use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordKind {
    Equal,
    Removed,
    Added,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSpan {
    pub kind: WordKind,
    pub text: String,
}

pub fn word_diff(old: &str, new: &str) -> Vec<WordSpan> {
    let diff = similar::TextDiff::from_words(old, new);
    let mut spans: Vec<WordSpan> = Vec::new();

    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            similar::ChangeTag::Equal => WordKind::Equal,
            similar::ChangeTag::Delete => WordKind::Removed,
            similar::ChangeTag::Insert => WordKind::Added,
        };
        match spans.last_mut() {
            Some(last) if last.kind == kind => last.text.push_str(change.value()),
            _ => spans.push(WordSpan {
                kind,
                text: change.value().to_string(),
            }),
        }
    }

    spans
}

pub fn emphasis_removed(spans: &[WordSpan]) -> Vec<bool> {
    emphasis(spans, WordKind::Removed)
}

pub fn emphasis_added(spans: &[WordSpan]) -> Vec<bool> {
    emphasis(spans, WordKind::Added)
}

fn emphasis(spans: &[WordSpan], mark: WordKind) -> Vec<bool> {
    let mut flags = Vec::new();
    for span in spans {
        let present = span.kind == WordKind::Equal || span.kind == mark;
        if present {
            let emphasized = span.kind == mark;
            flags.extend(std::iter::repeat_n(emphasized, span.text.chars().count()));
        }
    }
    flags
}

#[derive(Debug, Clone)]
pub struct HlSpan {
    pub color: (u8, u8, u8),
    pub text: String,
}

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();

pub fn highlighter() -> &'static Highlighter {
    HIGHLIGHTER.get_or_init(Highlighter::new)
}

impl Highlighter {
    fn new() -> Self {
        let syntaxes = SyntaxSet::load_defaults_nonewlines();
        let mut themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .remove("base16-ocean.dark")
            .or_else(|| themes.themes.values().next().cloned())
            .expect("syntect ships default themes");
        Self { syntaxes, theme }
    }

    pub fn language(&self, path: &str) -> Option<&SyntaxReference> {
        let extension = std::path::Path::new(path).extension()?.to_str()?;
        self.syntaxes.find_syntax_by_extension(extension)
    }

    pub fn highlight(&self, syntax: &SyntaxReference, line: &str) -> Vec<HlSpan> {
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        match highlighter.highlight_line(line, &self.syntaxes) {
            Ok(ranges) => ranges
                .into_iter()
                .map(|(style, text)| HlSpan {
                    color: (style.foreground.r, style.foreground.g, style.foreground.b),
                    text: text.to_string(),
                })
                .collect(),
            Err(_) => vec![HlSpan {
                color: (200, 200, 200),
                text: line.to_string(),
            }],
        }
    }
}
