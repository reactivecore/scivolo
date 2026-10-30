// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ImportInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! An import's selectors and what its `given` selectors import (dotty's `ImportInfo.givenBound`,
//! `importBound`).

use super::Typer;
use super::imports::{ImportEntry, Selector};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{List, NodeId, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// The selectors `selectors`, their bounds typed in `ctx`.
    pub fn selectors(&mut self, selectors: List, ctx: Ctx) -> Vec<Selector> {
        let items = self.ast.items(selectors).to_vec();
        let mut out = Vec::new();
        for s in items {
            let Some((name, rename)) = self.selector_names(s) else { continue };
            let Tree::ImportSelector { bound, .. } = *self.ast.tree(s) else { continue };
            let bound = bound.map(|b| self.typed_type(b, ctx));
            out.push(Selector { name, rename, bound });
        }
        out
    }

    /// The imported name and the name it is visible as of selector `s`.
    fn selector_names(&self, s: NodeId) -> Option<(String, String)> {
        let ident = |id: NodeId| match *self.ast.tree(id) {
            Tree::Ident { name } => self.text(name.name).to_owned(),
            _ => String::new(),
        };
        let Tree::ImportSelector { imported, renamed, .. } = *self.ast.tree(s) else { return None };
        let name = ident(imported);
        let rename = renamed.map_or_else(|| name.clone(), ident);
        Some((name, rename))
    }

    /// Whether import `i`, if its path is not typed yet, has a selector making a member visible
    /// as `name`.
    pub fn may_import_named(&self, i: usize, name: &str) -> bool {
        let ImportEntry::Pending { tree, .. } = self.imports[i] else { return true };
        let Tree::Import { selectors, .. } = *self.ast.tree(tree) else { return true };
        self.ast
            .items(selectors)
            .iter()
            .any(|&s| self.selector_names(s).is_some_and(|(_, rename)| rename == name && rename != "_"))
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
        let Some((binder, result, names)) = self.u.poly(info).map(|p| (p.binder, p.result, p.names())) else {
            return self.final_result(info);
        };
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
