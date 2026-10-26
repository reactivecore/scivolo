// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//! The type parameters of a secondary constructor, inside its body equal to those of its class
//! (dotty's `linkConstructorParams`).

use super::Typer;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Bounds each type parameter of secondary constructor `sym`, defined by `def`, by the type
    /// parameter of its class at the same position from both sides; the infos replaced.
    pub fn link_constructor_params(&mut self, sym: SymbolId, def: NodeId) -> Vec<(SymbolId, TypeId)> {
        let Some(class) = self.u.symbol(sym).owner else { return Vec::new() };
        if self.u.symbol(sym).name != "<init>" || self.syms.primary_ctors.get(&class) == Some(&sym) {
            return Vec::new();
        }
        let Some(first) = self.def_clauses(def).into_iter().next() else { return Vec::new() };
        let own: Vec<SymbolId> = first
            .iter()
            .filter(|&&p| matches!(self.ast.tree(p), Tree::TypeDef { .. }))
            .filter_map(|p| self.syms.by_tree.get(p).copied())
            .collect();
        let class_params = self.u.type_params(class);
        if own.len() != class_params.len() {
            return Vec::new();
        }
        for ctor in self.u.decl(class, "<init>").unwrap_or_default() {
            self.complete(ctor);
        }
        let class_ref = self.u.intern_type(Type::Sym(None, class));
        let this = self.u.intern_type(Type::This(class_ref));
        own.into_iter()
            .zip(class_params)
            .map(|(p, c)| {
                let saved = self.u.info(p);
                let linked = self.u.intern_type(Type::Sym(Some(this), c));
                let bounds = self.u.intern_type(Type::Bounds(linked, linked));
                self.u.set_info(p, bounds);
                (p, saved)
            })
            .collect()
    }

    /// Restores the infos `link_constructor_params` replaced.
    pub fn unlink_constructor_params(&mut self, saved: Vec<(SymbolId, TypeId)>) {
        for (p, info) in saved {
            self.u.set_info(p, info);
        }
    }
}
