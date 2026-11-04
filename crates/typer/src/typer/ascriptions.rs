// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Ascriptions of a block result's avoiding type (dotty's `ascribeType`), and conformance to the
//! recursive types avoiding local classes gives.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `expr: tpe`, or `expr.$asInstanceOf$[tpe]` if `expr` does not conform to `tpe`, moved into
    /// the result of a block (dotty's `ascribeType`).
    pub fn ascribed(&mut self, expr: TNodeId, tpe: TypeId) -> TNodeId {
        let span = self.tpd.span(expr);
        if let T::Block { stats, expr: last } = *self.tpd.tree(expr)
            && !matches!(self.tpd.tree(last), T::Closure { .. })
        {
            let last = self.ascribed(last, tpe);
            return self.build().block(stats, last, span);
        }
        let tpe = self.simplified(tpe);
        let tpt = self.type_tree(tpe, span.end_pos(), true);
        let actual = self.tpd.tpe(expr);
        if self.conforms_rec(actual, tpe) {
            return self.tpd.node(T::Typed { expr, tpt }, span.to_synthetic(), Some(tpe), None);
        }
        let any = self.u.builtins().any;
        let cast = self.u.decl(any, "$asInstanceOf$").ok().and_then(|d| d.first().copied());
        let Some(cast) = cast else {
            self.unsupported_at(span, "a cast without Any.$asInstanceOf$");
            return expr;
        };
        let select_tpe = self.u.intern_type(Type::Sym(Some(actual), cast));
        let name = TName { name: self.name("$asInstanceOf$"), is_type: false };
        let fun = self.tpd.node(T::Select { qualifier: expr, name }, span, Some(select_tpe), Some(cast));
        let args = self.tpd.list(&[tpt]);
        self.tpd.node(T::TypeApply { fun, args }, span, Some(tpe), None)
    }

    /// Whether `t` conforms to `target`, a recursive type in it compared with its `this` the
    /// skolem of `t`, or of a part of intersection `t` (dotty's `compareRec`).
    fn conforms_rec(&mut self, t: TypeId, target: TypeId) -> bool {
        if self.u.is_subtype(t, target) {
            return true;
        }
        if let Type::And(x, y) = *self.u.types().get(target) {
            return self.conforms_rec(t, x) && self.conforms_rec(t, y);
        }
        if let Type::And(x, y) = *self.u.types().get(t) {
            return self.conforms_rec(x, target) || self.conforms_rec(y, target);
        }
        let skolem = self.u.fresh_skolem(t);
        let opened = self.rec_this_as(target, skolem);
        opened != target && self.u.is_subtype(skolem, opened)
    }

    /// `t` with each recursive type replaced by its parent, its `this` by `skolem`.
    fn rec_this_as(&mut self, t: TypeId, skolem: TypeId) -> TypeId {
        self.u.map_type(t, &mut |u, part| {
            let Type::Rec(binder, parent) = *u.types().get(part) else { return None };
            Some(
                u.map_type(parent, &mut |u, x| {
                    (*u.types().get(x) == Type::RecThis(binder)).then_some(skolem)
                }),
            )
        })
    }

    /// `t` with each recursive type replaced by its parent without the refinements referring to
    /// its `this`.
    pub fn without_rec_refinements(&mut self, t: TypeId) -> TypeId {
        self.u.map_type(t, &mut |u, part| {
            let Type::Rec(binder, mut parent) = *u.types().get(part) else { return None };
            let mut kept = Vec::new();
            while let Type::Refined(p, name, info) = u.types().get(parent).clone() {
                let mut recursive = false;
                u.map_type(info, &mut |u, x| {
                    recursive |= *u.types().get(x) == Type::RecThis(binder);
                    None
                });
                if !recursive {
                    kept.push((name, info));
                }
                parent = p;
            }
            Some(kept.into_iter().rev().fold(parent, |p, (n, i)| u.intern_type(Type::Refined(p, n, i))))
        })
    }
}
