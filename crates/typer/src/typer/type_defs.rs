// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Type members, aliases and type parameters (dotty's `typedTypeDef`, `typedTypeBoundsTree`, and
//! the `TypeDefCompleter` side of their infos).

use super::Typer;
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The info of type definition `sym` with right-hand side `rhs`: bounds for an abstract type
    /// or a type parameter, else an alias; its typed tree is kept for typing `rhs`.
    pub fn type_def_info(&mut self, rhs: NodeId, ctx: Ctx) -> TypeId {
        let lambda = matches!(self.ast.tree(rhs), Tree::LambdaTypeTree { .. });
        let typed = self.typed_type(rhs, ctx);
        self.typed_ahead(rhs, typed, ctx);
        let tpe = self.tpd.tpe(typed);
        if !lambda && !matches!(self.u.types().get(tpe), Type::Bounds(..)) && self.type_arity(tpe) > 0 {
            self.unsupported(rhs, "alias of a type constructor");
            return self.u.intern_type(Type::Error);
        }
        match self.u.types().get(tpe) {
            Type::Bounds(..) => tpe,
            Type::Lambda { .. } if lambda => tpe,
            _ if matches!(self.ast.tree(rhs), Tree::DerivedTypeTree { .. }) => tpe,
            _ => self.u.intern_type(Type::Alias(tpe)),
        }
    }

    /// The info of type member or type parameter `sym` with right-hand side `rhs`.
    pub fn type_member_info(&mut self, sym: SymbolId, rhs: NodeId, ctx: Ctx) -> TypeId {
        match self.u.symbol(sym).flags.has(tasty::tags::OPAQUE) {
            true => self.opaque_info(sym, rhs, ctx),
            false => self.type_def_info(rhs, ctx),
        }
    }

    /// The info of opaque type `sym` with right-hand side `rhs`: its declared bounds, else
    /// `Nothing..Any`; its alias is recorded apart (dotty's `opaqueToBounds`).
    fn opaque_info(&mut self, sym: SymbolId, rhs: NodeId, ctx: Ctx) -> TypeId {
        let error = self.u.intern_type(Type::Error);
        if matches!(self.ast.tree(rhs), Tree::LambdaTypeTree { .. }) {
            self.unsupported(rhs, "higher-kinded opaque type");
            return error;
        }
        let typed = self.typed_type(rhs, ctx);
        self.typed_ahead(rhs, typed, ctx);
        let alias = self.tpd.tpe(typed);
        let (lo, hi) = match *self.tpd.tree(typed) {
            T::TypeBoundsTree { lo, hi, .. } => (self.tpd.tpe(lo), self.tpd.tpe(hi)),
            _ => (tasty::NOTHING, tasty::ANY),
        };
        if self.type_arity(alias) > 0 {
            self.unsupported(rhs, "opaque alias of a type constructor");
            return error;
        }
        if !self.u.is_subtype(lo, alias) || !self.u.is_subtype(alias, hi) {
            self.unsupported(rhs, "opaque alias outside its bounds");
            return error;
        }
        self.u.set_opaque_alias(sym, alias);
        self.u.intern_type(Type::Bounds(lo, hi))
    }

    pub fn typed_type_def(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(id) else { unreachable!() };
        let sym = self.syms.by_tree.get(&id).copied();
        if let Some(s) = sym {
            self.complete(s);
            let info = self.u.info(s);
            if self.u.symbol(s).kind == tasty::Kind::Type
                && let Type::Bounds(lo, hi) = *self.u.types().get(info)
                && !self.u.is_subtype(lo, hi)
            {
                return self.unsupported(id, "type member with conflicting bounds");
            }
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
