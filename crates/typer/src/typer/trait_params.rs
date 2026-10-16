//! Parent calls of parameterized traits (dotty's `ensureConstrCall` trait rule, `addUsingTraits`,
//! `checkParentCall` and `Mixin`'s check that such a trait is implemented directly).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// The primary constructor of class `c`.
    fn primary_ctor(&mut self, c: SymbolId) -> Option<SymbolId> {
        self.complete(c);
        if let Some(&ctor) = self.syms.primary_ctors.get(&c) {
            return Some(ctor);
        }
        self.u.decl(c, "<init>").ok()?.into_iter().next()
    }

    /// The value parameter clauses of the primary constructor of class `c`.
    fn ctor_clauses(&mut self, c: SymbolId) -> Vec<(bool, Vec<TypeId>)> {
        let Some(ctor) = self.primary_ctor(c) else { return Vec::new() };
        self.complete(ctor);
        let info = self.u.info(ctor);
        self.u.param_clauses(info)
    }

    /// Whether the primary constructor of trait `t` takes a parameter (dotty's `takesParams`).
    pub fn trait_takes_params(&mut self, t: SymbolId) -> bool {
        self.u.is_trait(t) && self.ctor_clauses(t).iter().any(|(_, ps)| !ps.is_empty())
    }

    /// Whether the primary constructor of trait `t` takes implicit parameters.
    fn takes_implicit_params(&mut self, t: SymbolId) -> bool {
        self.ctor_clauses(t).iter().any(|&(implicit, _)| implicit)
    }

    /// The superclass of class `cls`: the class of its first parent.
    fn super_class(&mut self, cls: SymbolId) -> Option<SymbolId> {
        let first = self.u.parents(cls).first().copied()?;
        self.u.class_symbol(first)
    }

    /// Whether the superclass of class `cls` derives from `base`.
    pub fn super_derives_from(&mut self, cls: SymbolId, base: SymbolId) -> bool {
        self.super_class(cls).is_some_and(|s| self.u.derives_from(s, base))
    }

    /// The traits class `cls` mixes in that its superclass does not (dotty's `mixins`).
    fn mixins(&mut self, cls: SymbolId) -> Vec<SymbolId> {
        let bases = self.u.linearization(cls);
        bases
            .into_iter()
            .skip(1)
            .filter(|&b| self.u.is_trait(b) && !self.super_derives_from(cls, b))
            .collect()
    }

    /// `parents` of class `cls` with, before each trait parent, the base traits of it taking
    /// implicit parameters that neither the superclass nor another parent gives (dotty's
    /// `addUsingTraits`).
    pub fn add_using_traits(&mut self, cls: SymbolId, parents: Vec<TypeId>) -> Vec<TypeId> {
        if self.u.is_trait(cls) {
            return parents;
        }
        let existing: Vec<Option<SymbolId>> = parents.iter().map(|&p| self.u.class_symbol(p)).collect();
        let super_class = existing.first().copied().flatten();
        let mut out = Vec::new();
        for &p in &parents {
            let Some(psym) = self.u.class_symbol(p).filter(|&s| self.u.is_trait(s)) else {
                out.push(p);
                continue;
            };
            let bases: Vec<SymbolId> =
                self.u.linearization(psym).into_iter().skip(1).take_while(|&b| self.u.is_trait(b)).collect();
            let mut added = Vec::new();
            for b in bases {
                let inherited = super_class.is_some_and(|s| self.u.derives_from(s, b));
                if self.takes_implicit_params(b) && !inherited && !existing.contains(&Some(b)) {
                    added.extend(self.u.base_type(p, b));
                }
            }
            out.extend(added.into_iter().rev());
            out.push(p);
        }
        out
    }

    /// Checks constructor call `call` of a parent of class `cls` (dotty's `checkParentCall`).
    pub fn check_parent_call(&mut self, cls: SymbolId, call: TNodeId, called: Option<SymbolId>) {
        let Some(called) = called else { return };
        let span = self.tpd.span(call);
        let (caller_name, called_name) =
            (self.u.symbol(cls).name.clone(), self.u.symbol(called).name.clone());
        if self.u.is_trait(cls) {
            self.error_at(span, format!("trait {caller_name} may not call constructor of {called_name}"));
        } else if self.u.is_trait(called) && !self.mixins(cls).contains(&called) {
            let sup = self.super_class(cls).map(|s| self.u.symbol(s).name.clone()).unwrap_or_default();
            let msg = format!(
                "trait {called_name} is already implemented by superclass {sup},\nits constructor cannot be called again"
            );
            self.error_at(span, msg);
        }
    }

    /// Checks that class `cls`, with parent trees `parents`, calls the constructor of each
    /// parameterized trait it mixes in (dotty's `traitInits`).
    pub fn check_trait_inits(&mut self, cls: SymbolId, parents: &[TNodeId], at: Span) {
        if self.u.is_trait(cls) {
            return;
        }
        let called: Vec<Option<SymbolId>> = parents.iter().map(|&p| self.called_class(p)).collect();
        for mixin in self.mixins(cls) {
            if self.trait_takes_params(mixin) && !called.contains(&Some(mixin)) {
                let name = self.u.symbol(mixin).name.clone();
                let msg = format!(
                    "parameterized trait {name} is indirectly implemented,\nneeds to be implemented directly so that arguments can be passed"
                );
                self.error_at(at, msg);
            }
        }
    }

    /// The class whose constructor parent tree `tree` calls, if it is a call.
    pub fn called_class(&mut self, mut tree: TNodeId) -> Option<SymbolId> {
        loop {
            match *self.tpd.tree(tree) {
                T::Apply { fun, .. } | T::TypeApply { fun, .. } => tree = fun,
                T::Block { expr, .. } => tree = expr,
                _ => break,
            }
        }
        let ctor = self.tpd.sym(tree)?;
        (self.u.symbol(ctor).name == "<init>").then(|| self.u.symbol(ctor).owner).flatten()
    }
}
