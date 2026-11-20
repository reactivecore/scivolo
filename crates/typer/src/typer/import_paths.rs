// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The paths of imports, typed when a lookup first needs them (dotty's `typedImport`).

use super::imports::{ImportEntry, ImportInfo};
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type};

impl Typer<'_> {
    /// Determines the info of import `i`, its path typed if not yet. A lookup through `i` while
    /// its path is typed, unless `wild_import_ref` skips `i`, types the path again once; `None`
    /// for a lookup through `i` during that typing. Refuses that lookup when the path
    /// selects a member of a term, as `import B.C.*` in an object that `B` exports (dotty's cyclic
    /// reference involving the import).
    pub fn import_info(&mut self, i: usize) -> Option<&ImportInfo> {
        let pending = match self.imports[i] {
            ImportEntry::Pending { tree, ctx } => Some((tree, ctx, false)),
            ImportEntry::Completing { tree, ctx, again: false } => Some((tree, ctx, true)),
            ImportEntry::Completing { .. } | ImportEntry::Done(_) => None,
        };
        if let Some((tree, ctx, again)) = pending {
            self.imports[i] = ImportEntry::Completing { tree, ctx, again };
            let outer = self.import_path_objects.replace(Vec::new());
            let info = self.typed_import_path(tree, ctx);
            let classes = std::mem::replace(&mut self.import_path_objects, outer).unwrap_or_default();
            if again && let Some(expr) = info.expr.filter(|&e| self.selects_from_term(e)) {
                self.unsupported_tree(self.tpd.span(expr), "cyclic reference involving an import");
            }
            self.imports[i] = ImportEntry::Done(info);
            self.complete_import_path_objects(classes);
        }
        match &self.imports[i] {
            ImportEntry::Done(info) => Some(info),
            _ => None,
        }
    }

    /// Completes `classes`, the classes of the objects on a typed import path, or leaves them to
    /// the import path around it.
    fn complete_import_path_objects(&mut self, classes: Vec<SymbolId>) {
        match &mut self.import_path_objects {
            Some(outer) => outer.extend(classes),
            None => classes.into_iter().for_each(|c| self.complete(c)),
        }
    }

    /// Whether typed path `path` selects a member of a term other than a package.
    fn selects_from_term(&self, path: TNodeId) -> bool {
        let T::Select { qualifier, .. } = *self.tpd.tree(path) else { return false };
        !self.tpd.sym(qualifier).is_some_and(|s| self.u.symbol(s).kind == tasty::Kind::Package)
    }

    fn typed_import_path(&mut self, tree: NodeId, ctx: Ctx) -> ImportInfo {
        let Tree::Import { expr, selectors } = *self.ast.tree(tree) else { unreachable!() };
        let typed = self.typed(expr, Pt::Any, ctx.with_mode(crate::ctx::Mode::QUALIFIER));
        let package = self.tpd.sym(typed).is_some_and(|s| self.u.symbol(s).kind == tasty::Kind::Package);
        let tpe = self.tpd.tpe(typed);
        if !package && !self.is_stable_path(typed) && !super::stability::is_stable_path(self.u, tpe) {
            self.check_stable_prefix(typed, super::type_prefix::IMPORT_PREFIX, ctx);
        } else if !package && !self.is_idempotent_expr(typed) && !matches!(self.u.ty(tpe), Type::Error) {
            self.error(expr, "import prefix is not a pure expression");
        }
        let qual = self.import_qual(typed);
        self.check_language_path(expr, qual);
        let selectors = self.selectors(selectors, ctx);
        let owner = Some(ctx.owner);
        ImportInfo { qual, selectors, expr: Some(typed), owner, tree: Some(tree) }
    }
}
