// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ImportInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! An import's selectors and what its `given` selectors import (dotty's `ImportInfo.givenBound`,
//! `importBound`).

use super::Typer;
use super::imports::Selector;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{List, NodeId, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// The selectors `selectors`, their bounds typed in `ctx`.
    pub fn selectors(&mut self, selectors: List, ctx: Ctx) -> Vec<Selector> {
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

    /// Whether a `given` selector of import `i` imports given or implicit `sym`: one without a
    /// bound, or one whose bound `sym`'s result type conforms to.
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
}
