// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Dynamic.scala
//! Structural refinements `{ val a: T; def m(x: A): R }` and selections of their members
//! (dotty's `handleStructural`).

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Selection `id` of member `name` of `qual`, which only a term refinement of its type `pre`
    /// declares, `qual` made a `Selectable`: a method left for `structural_call`, a value
    /// `qual.selectDynamic("name").$asInstanceOf$[T]`; `None` if no refinement declares it.
    pub fn structural_select(
        &mut self,
        id: NodeId,
        (qual, pre): (TNodeId, TypeId),
        name: TName,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let text = self.text(name.name).to_owned();
        if name.is_type {
            return None;
        }
        let info = self.u.term_refinement_named(pre, &text)?;
        let method = match self.u.types().get(info) {
            Type::Poly { .. } => return Some(self.unsupported(id, "polymorphic structural method")),
            Type::Method { .. } => true,
            _ => false,
        };
        let tpe = self.u.intern_type(Type::TermRef(pre, text.clone()));
        let span = self.ast.span(id);
        if method {
            return Some(self.structural_method(qual, name, info, span));
        }
        let qual = match self.selectable_qual(qual, ctx) {
            Ok(q) => q,
            Err(what) => return Some(self.unsupported(id, what)),
        };
        let (fun, literal) = self.structural_fun(qual, "selectDynamic", &text, span);
        let (apply, _) = self.applied(fun, &[literal]);
        let call = self.typed(apply, Pt::Any, ctx);
        Some(self.structural_cast(span, (qual, call), tpe, ctx))
    }

    /// `qual`, converted to a `Selectable` if it is none.
    pub fn selectable_qual(&mut self, qual: TNodeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let selectable = self.u.class_ref("scala.Selectable");
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        if self.u.is_subtype(qual_tpe, selectable) {
            return Ok(qual);
        }
        let converted = self.converted(qual, Goal::Conform(selectable), ctx)?;
        converted.ok_or("structural selection without reflectiveSelectable")
    }

    /// Whether `t` widens to a value class or a by-name one.
    pub fn is_value_class_type(&mut self, t: TypeId) -> bool {
        let widened = self.u.widen(t);
        let value = if let Type::ByName(r) = *self.u.types().get(widened) { r } else { widened };
        self.u.value_class(value).is_some()
    }

    /// `call.$asInstanceOf$[tpe]`, or `call` if it is an error; a value of value class type
    /// `tpe` read through a `reflect.Selectable` `qual` boxed first (dotty's `maybeBoxingCast`).
    pub fn structural_cast(
        &mut self,
        at: Span,
        (qual, call): (TNodeId, TNodeId),
        tpe: TypeId,
        ctx: Ctx,
    ) -> TNodeId {
        if self.is_error(call) {
            return call;
        }
        let reflective = self.u.class_ref("scala.reflect.Selectable");
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        let boxed = if self.is_value_class_type(tpe) && self.u.is_subtype(qual_tpe, reflective) {
            match self.structural_box(call, tpe, ctx) {
                Ok(boxed) => boxed,
                Err(what) => return self.unsupported_tree(at, what),
            }
        } else {
            call
        };
        self.cast_to_captured(boxed, tpe).unwrap_or_else(|what| self.unsupported_tree(at, what))
    }

    /// `new V(call.$asInstanceOf$[U])` for value class type `tpe`, `V`, of underlying type `U`.
    fn structural_box(&mut self, call: TNodeId, tpe: TypeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let widened = self.u.widen(tpe);
        let value = if let Type::ByName(r) = *self.u.types().get(widened) { r } else { widened };
        let value = self.u.dealias(value);
        let class = self.u.value_class(value).ok_or("structural member of an intersected value class")?;
        let field = self.u.underlying(class).ok_or("value class without an underlying value")?;
        let underlying = self.u.member_info(value, field);
        let underlying = self.final_result(underlying);
        let unboxed = self.cast_to_captured(call, underlying)?;
        let span = self.tpd.span(call);
        let start = span.start_pos().to_synthetic();
        let class_ref = crate::defn::type_ref(self.u, class);
        let tpt = self.type_tree(class_ref, start, false);
        let new = self.tpd.node(T::New { tpt }, start, Some(class_ref), None);
        let new = self.spliced(new);
        let init = TName { name: self.name("<init>"), is_type: false };
        let mut fun = self.ast.node(Tree::Select { qualifier: new, name: init });
        self.ast.with_span(fun, start);
        if let Type::Applied(_, targs) = self.u.types().get(value).clone() {
            let targs: Vec<NodeId> = targs
                .into_iter()
                .map(|a| {
                    let tree = self.type_tree(a, start, false);
                    self.spliced(tree)
                })
                .collect();
            let args = self.ast.list(&targs);
            fun = self.ast.node(Tree::TypeApply { fun, args });
            self.ast.with_span(fun, start);
        }
        let arg = self.spliced(unboxed);
        let args = self.ast.list(&[arg]);
        let apply = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(apply, span);
        Ok(self.typed(apply, Pt::Any, ctx))
    }
}
