// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Refinements of class parents under `experimental.modularity`, as in `given C is M`: split
//! off the parents and entered as members of the class.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Span, TName};
use syntax::config::Features;
use tasty::{Kind, NameId, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Determines parent type `parent` of class `cls`, from parent tree `p`, without its
    /// refinements, and enters a member for each refined type the class does not declare, its
    /// info the refinement's, intersected with that of an earlier parent (dotty's
    /// `separateRefinements` and `enterParentRefinementSyms`). A refined value, as tracked
    /// parameter `d` in `class Bar extends Foo("bar")`, becomes a deferred tracked value. Refuses a
    /// refined method the class does not declare.
    pub fn separate_parent_refinements(
        &mut self,
        cls: SymbolId,
        p: NodeId,
        parent: TypeId,
        ctx: Ctx,
    ) -> TypeId {
        if !matches!(self.u.ty(parent), Type::Refined(..) | Type::Annotated(..))
            || !self.feature_enabled(Features::MODULARITY, "experimental.modularity", ctx)
        {
            return parent;
        }
        let mut refinements: Vec<(NameId, TypeId)> = Vec::new();
        let parent = self.separated(parent, &mut refinements);
        for (name, info) in refinements {
            let text = self.u.name(name).to_owned();
            let is_type = matches!(self.u.ty(info), Type::Alias(_) | Type::Bounds(..));
            let entered = self.syms.parent_refinements.get(&cls).cloned().unwrap_or_default();
            if let Some(prev) = entered.into_iter().find(|&s| self.u.symbol(s).name == text) {
                let prev_info = self.u.info(prev);
                let glb = self.u.glb(prev_info, info);
                self.u.set_info(prev, glb);
                continue;
            }
            if self.u.decls_named(cls, &text).iter().any(|&d| self.u.is_type_symbol(d) == is_type) {
                continue;
            }
            if matches!(self.u.ty(info), Type::Method { .. } | Type::Poly { .. }) {
                self.unsupported(p, "parent refinement of a method");
                continue;
            }
            let (kind, flags) = match is_type {
                true => (Kind::Type, f::SYNTHETIC.or(f::DEFERRED)),
                false => (Kind::Val, f::SYNTHETIC.or(f::DEFERRED).or(f::TRACKED)),
            };
            let sym = self.syms.define(self.u, &text, kind, cls, flags);
            self.u.set_info(sym, info);
            self.syms.parent_refinements.entry(cls).or_default().push(sym);
        }
        parent
    }

    /// `t` without its refinements, which go to `out`, those of one name intersected.
    fn separated(&mut self, t: TypeId, out: &mut Vec<(NameId, TypeId)>) -> TypeId {
        match *self.u.ty(t) {
            Type::Refined(parent, name, info) => {
                let parent = self.separated(parent, out);
                let info = widen_skolems(self.u, info);
                match out.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, prev)) => *prev = self.u.glb(*prev, info),
                    None => out.push((name, info)),
                }
                parent
            }
            Type::Annotated(inner, annot) => {
                let inner = self.separated(inner, out);
                self.u.intern_type(Type::Annotated(inner, annot))
            }
            _ => t,
        }
    }

    /// The class of refined parent type `t`.
    pub fn refined_class(&mut self, t: TypeId) -> Option<SymbolId> {
        let dealiased = self.u.dealias(t);
        let separated = self.separated(dealiased, &mut Vec::new());
        self.u.class_symbol(separated)
    }

    /// The definitions of the members class `cls` has for its parents' refinements, at `span`
    /// (dotty's `addParentRefinements`).
    pub fn parent_refinement_defs(&mut self, cls: SymbolId, span: Span) -> Vec<TNodeId> {
        let syms = self.syms.parent_refinements.get(&cls).cloned().unwrap_or_default();
        let at = Span::at(span.start());
        syms.into_iter()
            .map(|s| {
                let info = self.u.info(s);
                let tpt = self.type_tree(info, at, false);
                let is_type = self.u.is_type_symbol(s);
                let name = TName { name: self.name(&self.u.symbol(s).name.clone()), is_type };
                let tree = match is_type {
                    true => T::TypeDef { name, rhs: tpt },
                    false => T::ValDef { name, tpt, rhs: None },
                };
                self.tpd.node(tree, at, None, Some(s))
            })
            .collect()
    }
}

/// `t` with its skolems replaced by their types (dotty's `widenSkolems`).
fn widen_skolems(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| match *u.ty(part) {
        Type::Skolem(_, info) => Some(widen_skolems(u, info)),
        _ => None,
    })
}
