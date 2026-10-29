//! Typing contexts (dotty's `Context` as far as typing reads it): the owner, the mode, and a
//! chain of scopes and imports that `find_ref` walks outward.

use tasty::SymbolId;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LevelId(u32);

/// Typing modes (dotty's `Mode`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Mode(u32);

impl Mode {
    pub const NONE: Mode = Mode(0);
    pub const PATTERN: Mode = Mode(1);
    /// A path that is the qualifier of a selection or an import, where a package may stand.
    pub const QUALIFIER: Mode = Mode(2);
    /// A search for a reflexive `CanEqual` instance, as under `strictEquality`.
    pub const STRICT_EQUALITY: Mode = Mode(4);
    /// The type argument of `constValue` or `constValueOpt`, where a compile-time operation type
    /// may stand.
    pub const CONST_VALUE_ARG: Mode = Mode(8);
    /// The type tree of a typed pattern, whose wildcard arguments become pattern-bound types.
    pub const PATTERN_TYPE: Mode = Mode(16);
    /// The qualifier of an `apply` selection, itself and not the trees in it.
    pub const APPLY_QUALIFIER: Mode = Mode(32);
    /// The left-hand side of an assignment, where a context function value stays unapplied.
    pub const ASSIGNMENT_LHS: Mode = Mode(64);
    /// The argument of a top-level splice of an inline method, where quotes may stand.
    pub const SPLICE: Mode = Mode(128);
    /// A case of an inline match, whose patterns the inliner matches at the call.
    pub const INLINE_CASE: Mode = Mode(256);
    /// The name of an extractor, which refers to no method (dotty's `UnapplySelectionProto`).
    pub const EXTRACTOR: Mode = Mode(512);
    /// The name of an extension method applied to its receiver, which refers to extension methods
    /// only (dotty's `findRef` with `ExtensionMethod` required).
    pub const EXTENSION_METHOD: Mode = Mode(1024);

    pub fn is(self, m: Mode) -> bool {
        self.0 & m.0 != 0
    }

    pub fn with(self, m: Mode) -> Mode {
        Mode(self.0 | m.0)
    }

    pub fn without(self, m: Mode) -> Mode {
        Mode(self.0 & !m.0)
    }
}

/// What a level of the context chain makes visible.
pub enum Level {
    /// The symbols entered in a block, a method's parameters or a pattern, by name.
    Scope(Vec<(String, SymbolId)>),
    /// The members of a class or package, `owner`.
    Members(SymbolId),
    /// An import (dotty's `ImportInfo`), by its index in the typer's imports.
    Import(usize),
}

struct Entry {
    outer: Option<LevelId>,
    level: Level,
}

/// The levels of all contexts of a unit.
#[derive(Default)]
pub struct Levels {
    all: Vec<Entry>,
}

impl Levels {
    pub fn push(&mut self, outer: Option<LevelId>, level: Level) -> LevelId {
        self.all.push(Entry { outer, level });
        LevelId(self.all.len() as u32 - 1)
    }

    pub fn get(&self, id: LevelId) -> &Level {
        &self.all[id.0 as usize].level
    }

    pub fn outer(&self, id: LevelId) -> Option<LevelId> {
        self.all[id.0 as usize].outer
    }

    /// The level whose definitions an import at `id` shares a scope with: `id` itself or the next
    /// scope or members level outward (dotty's `ctx.scope`).
    pub fn def_scope(&self, id: LevelId) -> Option<LevelId> {
        let mut l = Some(id);
        while let Some(i) = l {
            if !matches!(self.get(i), Level::Import(_)) {
                return Some(i);
            }
            l = self.outer(i);
        }
        None
    }

    /// The symbols entered in scope level `id`.
    pub fn scope_symbols(&self, id: LevelId) -> Vec<SymbolId> {
        match self.get(id) {
            Level::Scope(entries) => entries.iter().map(|&(_, s)| s).collect(),
            _ => Vec::new(),
        }
    }

    /// The symbol entered as `name` that `keep` accepts in the scope level `sym` is entered in.
    pub fn scope_mate(&self, sym: SymbolId, name: &str, keep: impl Fn(SymbolId) -> bool) -> Option<SymbolId> {
        let entries = self.all.iter().find_map(|e| match &e.level {
            Level::Scope(entries) if entries.iter().any(|&(_, s)| s == sym) => Some(entries),
            _ => None,
        })?;
        entries.iter().find(|&&(ref n, s)| n == name && s != sym && keep(s)).map(|&(_, s)| s)
    }

    /// Enters `sym` as `name` in scope level `id`.
    pub fn enter(&mut self, id: LevelId, name: &str, sym: SymbolId) {
        if let Level::Scope(entries) = &mut self.all[id.0 as usize].level {
            entries.push((name.to_owned(), sym));
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub owner: SymbolId,
    pub level: LevelId,
    pub mode: Mode,
}

impl Ctx {
    pub fn with_owner(self, owner: SymbolId) -> Ctx {
        Ctx { owner, ..self }
    }

    pub fn with_mode(self, mode: Mode) -> Ctx {
        Ctx { mode, ..self }
    }
}
