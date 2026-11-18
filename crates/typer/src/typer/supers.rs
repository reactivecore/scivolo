// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! `super` and `C.super[P]` (dotty's `typedSuper`, `TypeAssigner.superType`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_super(&mut self, id: NodeId, qual: NodeId, mix: Option<NodeId>, ctx: Ctx) -> TNodeId {
        let Tree::This { qual: this_qual } = *self.ast.tree(qual) else {
            return self.unsupported(id, "Super");
        };
        let this = self.typed_this(qual, this_qual, ctx);
        if self.is_error(this) {
            return this;
        }
        let Some(cls) = self.tpd.sym(this) else { return self.unsupported(id, "Super") };
        self.check_super_in_inline(id, cls, ctx);
        let (name, span) = match mix.map(|m| (self.ast.tree(m).clone(), self.ast.span(m))) {
            Some((Tree::Ident { name }, span)) => (name, span),
            Some(_) => return self.unsupported(id, "Super"),
            None => (TName { name: self.name(""), is_type: true }, Span::NONE),
        };
        let owntype = match mix {
            Some(_) => {
                let text = self.text(name.name).to_owned();
                match self.mixin_super(cls, &text) {
                    Some(p) => p,
                    None => return self.unsupported(id, "super qualifier that is not a unique parent"),
                }
            }
            None => self.all_parents(cls),
        };
        let mix = self.tpd.node(T::Ident { name }, span, None, None);
        let this_tpe = self.tpd.tpe(this);
        let tpe = self.u.intern_type(Type::Super(this_tpe, owntype));
        self.tpd.node(T::Super { qual: this, mix }, self.ast.span(id), Some(tpe), Some(cls))
    }

    /// Types `qualifier` of a selection of `name`: `super` before a type name as its `this`, the
    /// type being the same (dotty's `typedSuper` with a `SelectionProto` of a type name).
    pub fn typed_qualifier(&mut self, qualifier: NodeId, name: TName, ctx: Ctx) -> TNodeId {
        let Tree::Super { qual, .. } = *self.ast.tree(qualifier) else {
            return self.typed(qualifier, Pt::Any, ctx);
        };
        if !name.is_type {
            return self.typed(qualifier, Pt::Any, ctx);
        }
        let this = self.typed(qual, Pt::Any, ctx);
        if let Some(cls) = self.tpd.sym(this) {
            self.check_super_in_inline(qualifier, cls, ctx);
        }
        this
    }

    /// Reports `super` tree `id` of class `cls` if an inline method encloses it but not `cls`.
    fn check_super_in_inline(&mut self, id: NodeId, cls: SymbolId, ctx: Ctx) {
        if let Some(inline) = self.enclosing_inline(ctx.owner)
            && !self.is_inside(cls, inline)
        {
            let method = self.syms.described(self.u, inline);
            self.error_at(self.ast.span(id), format!("Super call not allowed in inlineable {method}"));
        }
    }

    /// Whether class `cls` is defined inside method `method` (dotty's `isLocal`).
    fn is_inside(&self, cls: SymbolId, method: SymbolId) -> bool {
        let mut o = self.u.symbol(cls).owner;
        while let Some(s) = o {
            if s == method {
                return true;
            }
            o = self.u.symbol(s).owner;
        }
        false
    }

    /// The type constructor of the one parent of `cls` whose class is named `name`.
    fn mixin_super(&mut self, cls: SymbolId, name: &str) -> Option<TypeId> {
        self.complete(cls);
        let mut found = self
            .u
            .parents(cls)
            .into_iter()
            .filter(|&p| self.u.class_symbol(p).is_some_and(|c| self.u.symbol(c).name == name));
        let p = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some(self.u.applied(p).map_or(p, |(tycon, _)| tycon))
    }

    /// The parents of `cls` intersected from the left, `Any` if it has none.
    fn all_parents(&mut self, cls: SymbolId) -> TypeId {
        self.complete(cls);
        let parents = self.u.parents(cls);
        let mut it = parents.into_iter();
        let Some(first) = it.next() else { return tasty::ANY };
        it.fold(first, |acc, p| self.u.intern_type(Type::And(acc, p)))
    }
}
