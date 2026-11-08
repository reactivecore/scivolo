// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The parent types of a class when it is completed (dotty's `parentTypes`,
//! `ensureFirstIsClass`, `adjustForBoxedUnit`).

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::defn::type_ref;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type tree of a parent: the class of a constructor call, whose `new` it records as a
    /// parent's, or the parent itself.
    fn parent_tpt(&mut self, parent: NodeId) -> NodeId {
        match *self.ast.tree(parent) {
            Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => self.parent_tpt(fun),
            Tree::Select { qualifier, name } if self.text(name.name) == "<init>" => {
                self.parent_tpt(qualifier)
            }
            Tree::New { tpt } => {
                self.parent_news.insert(parent);
                tpt
            }
            _ => parent,
        }
    }

    /// Whether constructor call `parent` has type arguments.
    fn has_type_args(&self, parent: NodeId) -> bool {
        match *self.ast.tree(parent) {
            Tree::TypeApply { .. } => true,
            Tree::Apply { fun, .. } => self.has_type_args(fun),
            _ => false,
        }
    }

    /// Reports implicit or given object `cls`, defined by `tree`, whose class parent `parent`
    /// has its type arguments or tracked parameters inferred (dotty's `missingType`).
    fn check_inferred_parent(&mut self, cls: SymbolId, tree: NodeId, parent: NodeId) {
        let implicit = |s: SymbolId, u: &tasty::Universe| {
            let flags = u.symbol(s).flags;
            flags.has(tasty::tags::IMPLICIT) || flags.has(tasty::tags::GIVEN)
        };
        if self.has_type_args(parent) || !self.u.is_module_class(cls) {
            return;
        }
        if self.u.source_module(cls).is_some_and(|m| implicit(m, self.u)) {
            let msg = "parent type of implicit definition needs to be given explicitly";
            self.error_at(self.ast.span(tree), msg);
        }
    }

    /// The parent types of class `cls`, defined by `tree`, from parent trees `parents`, typed in
    /// `ctx` and dealiased, with a class first.
    pub fn parent_types(&mut self, cls: SymbolId, tree: NodeId, parents: &[NodeId], ctx: Ctx) -> Vec<TypeId> {
        let mut types = Vec::new();
        for &p in parents {
            let tpt = self.parent_tpt(p);
            let typed = self.typed_type(tpt, ctx.with_mode(ctx.mode.with(Mode::PARENT_TYPE)));
            let mut tpe = self.tpd.tpe(typed);
            let dealiased = self.u.dealias(tpe);
            let dependent = self
                .u
                .class_symbol(dealiased)
                .is_some_and(|c| self.u.symbol(c).flags.has(tasty::tags::TRACKED));
            if matches!(self.ast.tree(p), Tree::Apply { .. }) && (self.type_arity(tpe) > 0 || dependent) {
                self.check_inferred_parent(cls, tree, p);
                let call = self.typed(p, super::Pt::Any, ctx);
                self.typed_ahead(p, call, ctx);
                tpe = self.tpd.tpe(call);
                tpe = match self.fully_defined_as(tpe, true) {
                    Ok(t) => t,
                    Err(what) => {
                        self.unsupported(p, what);
                        tpe
                    }
                };
            }
            let trait_ctor = self.u.class_symbol(tpe).is_some_and(|c| self.u.is_trait(c));
            if !matches!(self.ast.tree(p), Tree::Apply { .. }) && trait_ctor && self.type_arity(tpe) > 0 {
                let call = self.parent_new_call(typed, ctx);
                self.typed_ahead(p, call, ctx);
                let called = self.tpd.tpe(call);
                tpe = self.fully_defined_as(called, true).unwrap_or_else(|what| {
                    self.unsupported(p, what);
                    called
                });
            }
            let tpe = self.u.dealias(tpe);
            let tycon = match *self.u.types().get(tpe) {
                Type::Applied(tycon, _) => tycon,
                _ => tpe,
            };
            let alias = matches!(*self.u.types().get(tycon), Type::Sym(_, s) if !self.u.is_class(s));
            if alias && self.is_completing_type(tycon) {
                self.unsupported(p, "cyclic reference");
            }
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
                let ctors = if self.u.is_trait(c) {
                    self.u.decl(c, "<init>").unwrap_or_default()
                } else {
                    Vec::new()
                };
                if ctors.into_iter().any(|k| !self.is_accessible(k, ctx)) {
                    self.unsupported(p, "inaccessible trait constructor");
                }
            }
            types.push(tpe);
        }
        let types = self.ensure_first_is_class(types);
        let mut types = self.add_using_traits(cls, types);
        if self.u.full_name(cls) == "scala.runtime.BoxedUnit" {
            types.retain(|&t| {
                self.u.class_symbol(t).is_none_or(|c| self.u.full_name(c) != "java.io.Serializable")
            });
        }
        types
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

    /// Reports trait `cls` of definition `tree`, whose parents are set now, if its class parent
    /// was taken as `Object` while they were completed and is another class.
    pub fn check_object_parent_assumed(&mut self, cls: SymbolId, tree: NodeId) {
        if !self.object_parents_assumed.contains(&cls) {
            return;
        }
        self.object_parents_assumed.retain(|&c| c != cls);
        let first = self.u.parents(cls).first().and_then(|&p| self.u.class_symbol(p));
        if self.real_class_parent(first) != self.defn.object {
            self.unsupported(tree, "class parent of a trait whose parents were being completed");
        }
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
        if let Some(crate::symbols::Completion::Completing(_)) = self.syms.completions.get(&sym)
            && self.u.parents(sym).is_empty()
        {
            self.object_parents_assumed.push(sym);
        }
        let first = self.u.parents(sym).first().copied();
        let parent = first.and_then(|p| self.u.class_symbol(p));
        match parent {
            Some(_) => self.real_class_parent(parent),
            None => self.defn.object,
        }
    }
}
