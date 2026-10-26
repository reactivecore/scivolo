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
        bounds.into_iter().flatten().any(|b| {
            let bound = self.tpd.tpe(b);
            let (fits, _, _) = self.speculatively(|t| {
                let result = t.instantiated_result(sym, info);
                t.u.is_subtype_in(result, bound, &mut t.constraint)
            });
            fits
        })
    }

    /// The final result type of method type `info` of `sym`, its type parameters replaced by
    /// fresh type variables (dotty's `normalizedCompatible`).
    fn instantiated_result(&mut self, sym: SymbolId, info: tasty::TypeId) -> tasty::TypeId {
        let tasty::Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return self.final_result(info);
        };
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let vars: Vec<tasty::TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let result = self.u.subst(result, binder, &names, &vars);
        let tparams = self.u.type_params(sym);
        let result = self.u.map_type(result, &mut |u, part| match *u.types().get(part) {
            tasty::Type::Sym(_, s) => tparams.iter().position(|&p| p == s).and_then(|i| vars.get(i).copied()),
            _ => None,
        });
        self.final_result(result)
    }
}
