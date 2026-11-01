use crate::ast::span::Span;
use crate::names::Name;
use crate::source::Offset;
use rustc_hash::{FxHashMap, FxHashSet};

/// The `$N$` names made, and those of each pattern definition and `for` by the offset where
/// its tree starts: met when the namer enters a statement there, or the typer types a tree
/// there.
#[derive(Default)]
pub struct Uniques {
    made: Vec<Name>,
    set: FxHashSet<Name>,
    on_entry: FxHashMap<Offset, Vec<Name>>,
    on_typing: FxHashMap<Offset, Vec<Name>>,
}

impl Uniques {
    pub fn push(&mut self, name: Name) {
        self.made.push(name);
        self.set.insert(name);
    }

    /// How many names were made.
    pub fn made(&self) -> usize {
        self.made.len()
    }

    /// The names made since `made` was `from`, met when the namer enters a statement that starts
    /// where `span` does.
    pub fn group_on_entry(&mut self, from: usize, span: Span) {
        if span.exists() && from < self.made.len() {
            self.on_entry.entry(span.start()).or_default().extend(&self.made[from..]);
        }
    }

    /// The names made since `made` was `from`, met when the typer types a tree that starts where
    /// `span` does.
    pub fn group_on_typing(&mut self, from: usize, span: Span) {
        if span.exists() && from < self.made.len() {
            self.on_typing.entry(span.start()).or_default().extend(&self.made[from..]);
        }
    }

    pub fn contains(&self, name: Name) -> bool {
        self.set.contains(&name)
    }

    pub fn on_entry(&self, at: Offset) -> Option<&[Name]> {
        self.on_entry.get(&at).map(Vec::as_slice)
    }

    pub fn on_typing(&self, at: Offset) -> Option<&[Name]> {
        self.on_typing.get(&at).map(Vec::as_slice)
    }

    /// Moves the offsets of the groups by `by`, as `Ast::shift_spans` the trees.
    pub fn shift(&mut self, by: Offset) {
        for groups in [&mut self.on_entry, &mut self.on_typing] {
            *groups = std::mem::take(groups).into_iter().map(|(at, g)| (at + by, g)).collect();
        }
    }

    /// Takes the names and groups of `other`.
    pub fn include(&mut self, other: &Uniques) {
        self.set.extend(&other.set);
        for (mine, theirs) in [(&mut self.on_entry, &other.on_entry), (&mut self.on_typing, &other.on_typing)]
        {
            for (&at, g) in theirs {
                mine.entry(at).or_default().extend(g);
            }
        }
    }
}
