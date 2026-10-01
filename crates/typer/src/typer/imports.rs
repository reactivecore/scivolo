//! Imports: what an import makes visible (dotty's `ImportInfo`, `typedImport`, and the import
//! side of `findRef`: `namedImportRef`, `wildImportRef`, `selection`).

use super::idents::Found;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, LevelId};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, TName, Tree};
use tasty::{SymbolId, TypeId};

/// A selector `name => rename`; `name` empty for `given`, `rename` `_` for an unimport.
pub struct Selector {
    pub name: String,
    pub rename: String,
}

pub struct ImportInfo {
    /// The type of the imported path.
    pub qual: TypeId,
    pub selectors: Vec<Selector>,
    /// The typed path; `None` for a root import.
    pub expr: Option<TNodeId>,
}

impl ImportInfo {
    fn has(&self, name: &str) -> bool {
        self.selectors.iter().any(|s| s.name == name)
    }

    /// Whether a member named `name` is imported by a wildcard: not renamed by a selector
    /// (dotty's `isWildcardImport` and `excluded`).
    fn wildcard_imports(&self, name: &str) -> bool {
        let renamed = self.selectors.iter().any(|s| s.name == name && s.rename != s.name);
        (self.has("_") || self.has("")) && name != "<init>" && !renamed
    }
}

/// An import of a unit: its tree and the context its path is typed in, or what it imports.
pub enum ImportEntry {
    Pending { tree: NodeId, ctx: Ctx },
    Completing,
    Done(ImportInfo),
}

impl Typer<'_> {
    /// A level for import `tree`, whose path is typed in `ctx` when first needed.
    pub fn import_level(&mut self, tree: NodeId, ctx: Ctx) -> LevelId {
        self.imports.push(ImportEntry::Pending { tree, ctx });
        let level = self.levels.push(Some(ctx.level), Level::Import(self.imports.len() - 1));
        self.import_levels.insert(tree, level);
        level
    }

    /// A root import of all members of `qual`.
    pub fn root_import(&mut self, outer: Option<LevelId>, qual: TypeId) -> LevelId {
        let selectors = vec![Selector { name: "_".into(), rename: "_".into() }];
        self.imports.push(ImportEntry::Done(ImportInfo { qual, selectors, expr: None }));
        self.levels.push(outer, Level::Import(self.imports.len() - 1))
    }

    /// Whether import `i` is one of the imports every unit starts with.
    pub fn is_root_import(&self, i: usize) -> bool {
        matches!(&self.imports[i], ImportEntry::Done(ImportInfo { expr: None, .. }))
    }

    /// The info of import `i`, its path typed if not yet.
    pub fn import_info(&mut self, i: usize) -> Option<&ImportInfo> {
        if let ImportEntry::Pending { tree, ctx } = self.imports[i] {
            self.imports[i] = ImportEntry::Completing;
            let Tree::Import { expr, selectors } = *self.ast.tree(tree) else { unreachable!() };
            let typed = self.typed(expr, Pt::Any, ctx);
            let qual = self.tpd.tpe(typed);
            let selectors = self.selectors(selectors);
            self.imports[i] = ImportEntry::Done(ImportInfo { qual, selectors, expr: Some(typed) });
        }
        match &self.imports[i] {
            ImportEntry::Done(info) => Some(info),
            _ => None,
        }
    }

    fn selectors(&self, selectors: List) -> Vec<Selector> {
        let ident = |id: NodeId| match *self.ast.tree(id) {
            Tree::Ident { name } => self.text(name.name).to_owned(),
            _ => String::new(),
        };
        let items = self.ast.items(selectors);
        let selectors = items.iter().filter_map(|&s| match *self.ast.tree(s) {
            Tree::ImportSelector { imported, renamed, .. } => {
                let name = ident(imported);
                let rename = renamed.map_or_else(|| name.clone(), ident);
                Some(Selector { name, rename })
            }
            _ => None,
        });
        selectors.collect()
    }

    /// The members named `member` of import `i`'s path; for a wildcard, the givens only if it is
    /// a `given` wildcard and the others only if it is a `_` wildcard (dotty's `selection`).
    fn selection(&mut self, i: usize, member: &str, is_type: bool, wildcard: bool) -> Option<Found> {
        let info = self.import_info(i)?;
        let (qual, given_wild, plain_wild) = (info.qual, info.has(""), info.has("_"));
        let syms: Vec<SymbolId> = self
            .type_members(qual, member)
            .into_iter()
            .filter(|&s| self.is_type_symbol(s) == is_type)
            .filter(|&s| {
                let given = self.u.symbol(s).flags.has(tasty::tags::GIVEN);
                !wildcard || if given { given_wild } else { plain_wild }
            })
            .collect();
        (!syms.is_empty()).then_some(Found { prefix: Some(qual), syms })
    }

    /// What import `i` names `name` explicitly, if anything (dotty's `namedImportRef`).
    pub fn named_import_ref(&mut self, i: usize, name: &str, is_type: bool) -> Option<Found> {
        let info = self.import_info(i)?;
        let member = info.selectors.iter().find(|s| s.rename == name && s.rename != "_")?.name.clone();
        self.selection(i, &member, is_type, false)
    }

    /// What import `i` makes visible as `name` by a wildcard (dotty's `wildImportRef`).
    pub fn wild_import_ref(&mut self, i: usize, name: &str, is_type: bool) -> Option<Found> {
        if !self.import_info(i)?.wildcard_imports(name) {
            return None;
        }
        self.selection(i, name, is_type, true)
    }

    /// The typed import `id`, its path typed when its level was first searched or now.
    pub fn typed_import(&mut self, id: NodeId, selectors: List) -> TNodeId {
        let level = self.import_levels[&id];
        let Level::Import(i) = *self.levels.get(level) else { unreachable!() };
        self.import_info(i);
        let ImportEntry::Done(ImportInfo { expr: Some(expr), .. }) = &self.imports[i] else {
            return self.unsupported(id, "cyclic import");
        };
        let expr = *expr;
        if let Err(what) = self.check_selectors(i) {
            return self.unsupported(id, what);
        }
        let items = self.ast.items(selectors).to_vec();
        let typed: Vec<TNodeId> = items.into_iter().map(|s| self.untyped_selector(s)).collect();
        let selectors = self.tpd.list(&typed);
        let span = self.ast.span(id);
        self.tpd.node(T::Import { expr, selectors }, span, None, None)
    }

    /// Checks that the named selectors of import `i` name members of its path, each once (dotty's
    /// `checkImportSelectors`).
    fn check_selectors(&mut self, i: usize) -> Result<(), &'static str> {
        let ImportEntry::Done(info) = &self.imports[i] else { return Ok(()) };
        let qual = info.qual;
        let names: Vec<String> =
            info.selectors.iter().map(|s| s.name.clone()).filter(|n| n != "_" && !n.is_empty()).collect();
        for (k, name) in names.iter().enumerate() {
            if names[..k].contains(name) {
                return Err("import selector repeated");
            }
            if self.type_members(qual, name).is_empty() {
                return Err("import of a missing member");
            }
        }
        Ok(())
    }

    /// An import selector as dotty keeps it: untyped identifiers.
    fn untyped_selector(&mut self, s: NodeId) -> TNodeId {
        let Tree::ImportSelector { imported, renamed, bound } = *self.ast.tree(s) else {
            return self.unsupported(s, "import selector");
        };
        if let Some(b) = bound {
            return self.unsupported(b, "import bound");
        }
        let ident = |t: &mut Self, id: NodeId| match *t.ast.tree(id) {
            Tree::Ident { name } => {
                let span = t.ast.span(id);
                t.tpd.node(T::Ident { name: TName { name: name.name, is_type: false } }, span, None, None)
            }
            _ => t.unsupported(id, "import selector"),
        };
        let imported = ident(self, imported);
        let renamed = renamed.map(|r| ident(self, r));
        let span = self.ast.span(s);
        self.tpd.node(T::ImportSelector { imported, renamed, bound: None }, span, None, None)
    }
}
