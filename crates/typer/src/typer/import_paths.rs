// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The paths of imports, typed when a lookup first needs them (dotty's `typedImport`).

use super::imports::{ImportEntry, ImportInfo};
use super::{Pt, Typer};
use crate::ctx::Ctx;
use syntax::ast::{NodeId, Tree};

impl Typer<'_> {
    /// Determines the info of import `i`, its path typed if not yet. A lookup through `i` while
    /// its path is typed, as when that completes an object whose exports refer to names `i`
    /// imports, types the path again once; `None` for a lookup through `i` during that typing.
    pub fn import_info(&mut self, i: usize) -> Option<&ImportInfo> {
        let pending = match self.imports[i] {
            ImportEntry::Pending { tree, ctx } => Some((tree, ctx, false)),
            ImportEntry::Completing { tree, ctx, again: false } => Some((tree, ctx, true)),
            ImportEntry::Completing { .. } | ImportEntry::Done(_) => None,
        };
        if let Some((tree, ctx, again)) = pending {
            self.imports[i] = ImportEntry::Completing { tree, ctx, again };
            let info = self.typed_import_path(tree, ctx);
            self.imports[i] = ImportEntry::Done(info);
        }
        match &self.imports[i] {
            ImportEntry::Done(info) => Some(info),
            _ => None,
        }
    }

    fn typed_import_path(&mut self, tree: NodeId, ctx: Ctx) -> ImportInfo {
        let Tree::Import { expr, selectors } = *self.ast.tree(tree) else { unreachable!() };
        let typed = self.typed(expr, Pt::Any, ctx.with_mode(crate::ctx::Mode::QUALIFIER));
        let package = self.tpd.sym(typed).is_some_and(|s| self.u.symbol(s).kind == tasty::Kind::Package);
        let tpe = self.tpd.tpe(typed);
        if !package && !self.is_stable_path(typed) && !super::stability::is_stable_path(self.u, tpe) {
            self.unsupported(expr, "import prefix that is not a pure expression");
        }
        let qual = self.import_qual(typed);
        self.check_language_path(expr, qual);
        let selectors = self.selectors(selectors, ctx);
        let owner = Some(ctx.owner);
        ImportInfo { qual, selectors, expr: Some(typed), owner, tree: Some(tree) }
    }
}
