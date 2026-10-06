//! Type members, aliases and type parameters (dotty's `typedTypeDef`, `typedTypeBoundsTree`, and
//! the `TypeDefCompleter` side of their infos).

use super::Typer;
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The info of type definition `sym` with right-hand side `rhs`: bounds for an abstract type
    /// or a type parameter, else an alias; its typed tree is kept for typing `rhs`.
    pub fn type_def_info(&mut self, rhs: NodeId, ctx: Ctx) -> TypeId {
        if matches!(self.ast.tree(rhs), Tree::LambdaTypeTree { .. }) {
            self.unsupported(rhs, "type lambda");
            return self.u.intern_type(Type::Error);
        }
        let typed = self.typed_type(rhs, ctx);
        self.typed_ahead(rhs, typed, ctx);
        let tpe = self.tpd.tpe(typed);
        match self.u.types().get(tpe) {
            Type::Bounds(..) => tpe,
            _ if matches!(self.ast.tree(rhs), Tree::DerivedTypeTree { .. }) => tpe,
            _ => self.u.intern_type(Type::Alias(tpe)),
        }
    }

    pub fn typed_type_def(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(id) else { unreachable!() };
        let sym = self.syms.by_tree.get(&id).copied();
        if let Some(s) = sym {
            self.complete(s);
        }
        let rhs = self.typed_type(rhs, ctx);
        let span = self.ast.span(id);
        self.tpd.node(T::TypeDef { name, rhs }, span, None, sym)
    }

    /// `>: lo <: hi`, a missing bound `Nothing` or `Any` placed as dotty's `envelope` places a
    /// child without a span: at the end of the children before it, else at the start of those
    /// after it, else at the bounds' start.
    pub fn typed_type_bounds(
        &mut self,
        id: NodeId,
        lo: Option<NodeId>,
        hi: Option<NodeId>,
        alias: Option<NodeId>,
        ctx: Ctx,
    ) -> TNodeId {
        let span = self.ast.span(id);
        let typed_lo = lo.map(|l| self.typed_type(l, ctx));
        let typed_hi = hi.map(|h| self.typed_type(h, ctx));
        let alias = alias.map(|a| self.typed_type(a, ctx));
        let after_lo = [typed_hi, alias].into_iter().flatten().map(|t| self.tpd.span(t)).find(|s| s.exists());
        let lo = match typed_lo {
            Some(l) => l,
            None => {
                let nothing = type_ref(self.u, self.u.builtins().nothing);
                self.type_tree(nothing, after_lo.unwrap_or(span).start_pos(), false)
            }
        };
        let hi = match typed_hi {
            Some(h) => h,
            None => {
                let any = type_ref(self.u, self.u.builtins().any);
                let lo_span = self.tpd.span(lo);
                let at =
                    if typed_lo.is_some() && lo_span.exists() { lo_span.end_pos() } else { span.start_pos() };
                self.type_tree(any, at, false)
            }
        };
        let tpe = match alias {
            Some(a) => self.tpd.tpe(a),
            None => {
                let (l, h) = (self.tpd.tpe(lo), self.tpd.tpe(hi));
                self.u.intern_type(Type::Bounds(l, h))
            }
        };
        self.tpd.node(T::TypeBoundsTree { lo, hi, alias }, span, Some(tpe), None)
    }
}
