// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! Trees of path types (dotty's `singleton` and `ref` of a term reference).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// A tree of path type `prefix` at `span`: `C.this`, or a reference to a stable term
    /// through its own prefix unless that can be left implicit; `None` if outside the subset.
    pub fn path_tree(&mut self, prefix: TypeId, span: Span, depth: u32) -> Option<TNodeId> {
        match self.u.types().get(prefix).clone() {
            Type::This(class) => Some(self.build().this(class, span)),
            Type::Sym(p, s) if !self.u.is_type_symbol(s) && self.is_stable_member(s) && depth > 0 => {
                let static_class =
                    |t: &mut Self, c| t.u.class_symbol(c).is_some_and(|cls| t.is_static_owner(cls));
                let elidable = match p.map(|p| self.u.types().get(p).clone()) {
                    None | Some(Type::NoPrefix | Type::Package(_)) => true,
                    Some(Type::This(c)) if self.is_param_or_accessor(s) => {
                        static_class(self, c).then_some(true)?
                    }
                    Some(Type::This(c)) => static_class(self, c),
                    Some(Type::Sym(_, o)) => {
                        self.u.symbol(o).flags.has(tasty::tags::OBJECT) && self.is_static_term(o)
                    }
                    _ => return None,
                };
                if elidable {
                    return Some(self.build().ident(s, prefix, span));
                }
                let qualifier = self.path_tree(p?, span, depth - 1)?;
                let name = self.build().term_name(s);
                Some(self.tpd.node(T::Select { qualifier, name }, span, Some(prefix), Some(s)))
            }
            _ => None,
        }
    }

    /// `path_tree` from `ctx`, where a parameter or accessor of the enclosing class is an
    /// identifier (dotty's `prefixIsElidable`).
    pub fn path_tree_in(&mut self, prefix: TypeId, span: Span, ctx: Ctx) -> Option<TNodeId> {
        if let Type::Sym(Some(p), s) = *self.u.types().get(prefix)
            && matches!(self.u.types().get(p), Type::This(_))
            && self.is_param_or_accessor(s)
            && self.term_prefix_is_elidable(Some(p), s, ctx)
        {
            return Some(self.build().ident(s, prefix, span));
        }
        self.path_tree(prefix, span, 8)
    }

    /// A reference of type `tpe` to member `sym` of `prefix` from `ctx` by its term name (dotty's
    /// `ref(tp)`): an identifier if the prefix can be left implicit, else `C.this.name`.
    pub fn ref_(
        &mut self,
        prefix: Option<TypeId>,
        sym: SymbolId,
        tpe: TypeId,
        span: Span,
        ctx: Ctx,
    ) -> TNodeId {
        if self.term_prefix_is_elidable(prefix, sym, ctx) {
            return self.build().ident(sym, tpe, span);
        }
        let name = self.build().term_name(sym);
        self.select_through(span, name, tpe, sym, Some(ctx))
    }

    /// `p.name` of type `tpe`, a reference to `sym` at `span` through the path `p` of its prefix
    /// (dotty's `ref(tpe)`).
    pub fn this_select(&mut self, span: Span, name: TName, tpe: TypeId, sym: SymbolId) -> TNodeId {
        self.select_through(span, name, tpe, sym, None)
    }

    /// `this_select`, its path built from `ctx` if given.
    fn select_through(
        &mut self,
        span: Span,
        name: TName,
        tpe: TypeId,
        sym: SymbolId,
        ctx: Option<Ctx>,
    ) -> TNodeId {
        let Type::Sym(Some(prefix), _) = *self.u.types().get(tpe) else { unreachable!("a member reference") };
        let path = match ctx {
            Some(ctx) => self.path_tree_in(prefix, span.start_pos(), ctx),
            None => self.path_tree(prefix, span.start_pos(), 8),
        };
        let Some(qualifier) = path else {
            self.unsupported_at(span, "reference through a path");
            return self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym));
        };
        let name = match self.u.types().get(prefix) {
            Type::This(_) => name,
            _ => TName { is_type: name.is_type, ..self.build().term_name(sym) },
        };
        self.tpd.node(T::Select { qualifier, name }, span, Some(tpe), Some(sym))
    }
}
