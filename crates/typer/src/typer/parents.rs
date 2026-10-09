//! The parent types of a class when it is completed (dotty's `parentTypes`,
//! `ensureFirstIsClass`).

use super::Typer;
use crate::ctx::Ctx;
use crate::defn::type_ref;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// The type tree of a parent: the class of a constructor call, or the parent itself.
    fn parent_tpt(&self, parent: NodeId) -> NodeId {
        match *self.ast.tree(parent) {
            Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => self.parent_tpt(fun),
            Tree::Select { qualifier, name } if self.text(name.name) == "<init>" => {
                self.parent_tpt(qualifier)
            }
            Tree::New { tpt } => tpt,
            _ => parent,
        }
    }

    /// The parent types of a class from parent trees `parents`, typed in `ctx` and dealiased,
    /// with a class first.
    pub fn parent_types(&mut self, parents: &[NodeId], ctx: Ctx) -> Vec<TypeId> {
        let mut types = Vec::new();
        for &p in parents {
            let tpt = self.parent_tpt(p);
            let typed = self.typed_type(tpt, ctx);
            let tpe = self.tpd.tpe(typed);
            let tpe = self.u.dealias(tpe);
            self.check_simple_kinded(tpe, tpt);
            if let Some(c) = self.u.class_symbol(tpe) {
                self.complete(c);
                if self.ast.span(p).is_source_derived()
                    && types.iter().any(|&t| self.u.class_symbol(t) == Some(c))
                {
                    self.unsupported(p, "class extended twice");
                }
                if !types.is_empty() && !self.u.is_trait(c) {
                    self.unsupported(p, "class used as a mixin");
                }
            }
            types.push(tpe);
        }
        self.ensure_first_is_class(types)
    }

    /// `parents` with the class parent their class parents agree on first, if they do not start
    /// with a class (dotty's `ensureFirstIsClass`).
    fn ensure_first_is_class(&mut self, parents: Vec<TypeId>) -> Vec<TypeId> {
        let first = parents.first().and_then(|&p| self.u.class_symbol(p));
        if first.is_some_and(|c| !self.u.is_trait(c)) {
            return parents;
        }
        let mut pcls = self.defn.object;
        for &p in &parents {
            let class = self.u.class_symbol(p);
            let candidate = self.real_class_parent(class);
            if self.u.derives_from(candidate, pcls) {
                pcls = candidate;
            }
        }
        let object = type_ref(self.u, self.defn.object);
        let mut first = object;
        for &p in &parents {
            if let Some(base) = self.u.base_type(p, pcls) {
                first = self.u.glb(first, base);
            }
        }
        let mut out = vec![first];
        out.extend(parents);
        out
    }

    /// The class `sym` is or, for a trait, the class its first parent leads to.
    fn real_class_parent(&mut self, sym: Option<SymbolId>) -> SymbolId {
        let Some(sym) = sym.filter(|&s| self.u.is_class(s)) else {
            return self.defn.object;
        };
        if !self.u.is_trait(sym) {
            return sym;
        }
        self.complete(sym);
        if let Some(&crate::symbols::Completion::Completing(tree)) = self.syms.completions.get(&sym) {
            self.unsupported(tree, "parents of a class whose parents are being completed");
        }
        let first = self.u.parents(sym).first().copied();
        let parent = first.and_then(|p| self.u.class_symbol(p));
        match parent {
            Some(_) => self.real_class_parent(parent),
            None => self.defn.object,
        }
    }
}
