//! `super` and `C.super[P]` (dotty's `typedSuper`, `TypeAssigner.superType`).

use super::Typer;
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
        Some(match *self.u.types().get(p) {
            Type::Applied(tycon, _) => tycon,
            _ => p,
        })
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
