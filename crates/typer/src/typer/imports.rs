// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ImportInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Imports: what an import makes visible (dotty's `ImportInfo`, `typedImport`, and the import
//! side of `findRef`: `namedImportRef`, `wildImportRef`, `selection`).

use super::idents::Found;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, LevelId};
use crate::tpd::TNodeId;
use syntax::ast::{List, NodeId, Tree};
use tasty::{SymbolId, TypeId};

/// A selector `name => rename`; `name` empty for `given`, `rename` `_` for an unimport; the
/// typed bound of a `given T`.
pub struct Selector {
    pub name: String,
    pub rename: String,
    pub bound: Option<TNodeId>,
}

pub struct ImportInfo {
    /// The type of the imported path.
    pub qual: TypeId,
    pub selectors: Vec<Selector>,
    /// The typed path; `None` for a root import.
    pub expr: Option<TNodeId>,
    /// The owner of the context the import is in; `None` for a root import.
    pub owner: Option<SymbolId>,
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
        let selector =
            |name: &str, rename: &str| Selector { name: name.into(), rename: rename.into(), bound: None };
        let mut selectors = vec![selector("_", "_"), selector("", "")];
        if self.import_site(qual) == Some(self.defn.predef) {
            selectors.insert(0, selector("any2stringadd", "_"));
        }
        self.imports.push(ImportEntry::Done(ImportInfo { qual, selectors, expr: None, owner: None }));
        self.levels.push(outer, Level::Import(self.imports.len() - 1))
    }

    /// The term symbol an import's path of type `qual` refers to.
    pub fn import_site(&self, qual: TypeId) -> Option<SymbolId> {
        match *self.u.types().get(qual) {
            tasty::Type::Sym(_, s) | tasty::Type::Package(s) => Some(s),
            _ => None,
        }
    }

    /// The root import that import `i` hides when it does not supply a name: `Predef`'s, if `i`
    /// imports from `Predef` (dotty's `ImportInfo.unimported`).
    pub fn unimported(&mut self, i: usize) -> Option<SymbolId> {
        if self.is_root_import(i) {
            return None;
        }
        let qual = self.import_info(i)?.qual;
        self.import_site(qual).filter(|&s| s == self.defn.predef)
    }

    /// The term symbol root import `i` imports from.
    pub fn root_site(&self, i: usize) -> Option<SymbolId> {
        match &self.imports[i] {
            ImportEntry::Done(info) => self.import_site(info.qual),
            _ => None,
        }
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
            let typed = self.typed(expr, Pt::Any, ctx.with_mode(crate::ctx::Mode::QUALIFIER));
            let package = self.tpd.sym(typed).is_some_and(|s| self.u.symbol(s).kind == tasty::Kind::Package);
            if !package && !self.is_stable_path(typed) {
                self.unsupported(expr, "import prefix that is not a pure expression");
            }
            let qual = self.tpd.tpe(typed);
            let site = self.import_site(qual).map(|s| self.u.full_name(s).replace('$', ""));
            if self.looks_like_language(expr) && !site.is_some_and(|s| s.starts_with("scala.language")) {
                self.unsupported(
                    expr,
                    "import that looks like a language import but refers to something else",
                );
            }
            let selectors = self.selectors(selectors, ctx);
            let owner = Some(ctx.owner);
            self.imports[i] = ImportEntry::Done(ImportInfo { qual, selectors, expr: Some(typed), owner });
        }
        match &self.imports[i] {
            ImportEntry::Done(info) => Some(info),
            _ => None,
        }
    }

    /// Whether path `expr` is `[[_root_.]scala.]language[.x]` (dotty's `languageImport`).
    fn looks_like_language(&self, expr: NodeId) -> bool {
        let mut names = Vec::new();
        let mut t = expr;
        loop {
            match *self.ast.tree(t) {
                Tree::Select { qualifier, name } => {
                    names.push(self.text(name.name).to_owned());
                    t = qualifier;
                }
                Tree::Ident { name } => {
                    names.push(self.text(name.name).to_owned());
                    break;
                }
                _ => return false,
            }
        }
        names.reverse();
        let rest = match names.iter().position(|n| n == "language") {
            Some(i) => &names[..i],
            None => return false,
        };
        rest.is_empty() || rest == ["scala"] || rest == ["_root_", "scala"]
    }

    fn selectors(&mut self, selectors: List, ctx: Ctx) -> Vec<Selector> {
        let ident = |t: &Self, id: NodeId| match *t.ast.tree(id) {
            Tree::Ident { name } => t.text(name.name).to_owned(),
            _ => String::new(),
        };
        let items = self.ast.items(selectors).to_vec();
        let mut out = Vec::new();
        for s in items {
            let Tree::ImportSelector { imported, renamed, bound } = *self.ast.tree(s) else { continue };
            let name = ident(self, imported);
            let rename = renamed.map_or_else(|| name.clone(), |r| ident(self, r));
            let bound = bound.map(|b| self.typed_type(b, ctx));
            out.push(Selector { name, rename, bound });
        }
        out
    }

    /// Whether a `given` selector of import `i` imports given `sym`: one without a bound, or
    /// one whose bound `sym`'s result type conforms to (dotty's `ImportInfo.givenBound`).
    pub fn imports_given(&mut self, i: usize, sym: SymbolId) -> bool {
        let Some(info) = self.import_info(i) else { return false };
        let qual = info.qual;
        let bounds: Vec<Option<TNodeId>> =
            info.selectors.iter().filter(|s| s.name.is_empty()).map(|s| s.bound).collect();
        if bounds.iter().any(Option::is_none) {
            return true;
        }
        if bounds.is_empty() {
            return false;
        }
        self.complete(sym);
        let info = self.u.member_info(qual, sym);
        let result = self.final_result(info);
        bounds.into_iter().flatten().any(|b| {
            let bound = self.tpd.tpe(b);
            self.u.is_subtype(result, bound)
        })
    }

    /// The members named `member` of import `i`'s path; for a wildcard, the givens only if a
    /// `given` selector imports them and the others only if it is a `_` wildcard (dotty's
    /// `selection`).
    fn selection(&mut self, i: usize, member: &str, is_type: bool, wildcard: bool) -> Option<Found> {
        let info = self.import_info(i)?;
        let (qual, plain_wild) = (info.qual, info.has("_"));
        let mut syms: Vec<SymbolId> = self.type_members(qual, member);
        syms.retain(|&s| self.u.is_type_symbol(s) == is_type);
        if wildcard {
            let mut kept = Vec::new();
            for s in syms {
                let given = self.u.symbol(s).flags.has(tasty::tags::GIVEN);
                if if given { self.imports_given(i, s) } else { plain_wild } {
                    kept.push(s);
                }
            }
            syms = kept;
        }
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
}
