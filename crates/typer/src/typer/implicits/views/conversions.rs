// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Applying one conversion candidate to the tree of a view search (dotty's `typedImplicit` of a
//! view).

use super::super::super::{Pt, Typer};
use super::super::candidates::{Candidate, candidate_ref};
use super::view_kinds::{ViewKind, view_kind};
use super::{Goal, ViewSearch};
use crate::tpd::TNodeId;
use tasty::{Binding, Type, TypeId};

impl ViewSearch<'_> {
    /// Applies conversion `c` to the tree with `c`'s implicit arguments if the result fits the
    /// goal, as `1` becomes `intToBig(1)`: a constant tree widened to the parameter type first if
    /// `widen`, the wildcards of its type captured where that makes it fit.
    pub fn try_conversion(
        &self,
        t: &mut Typer<'_>,
        c: Candidate,
        widen: bool,
    ) -> Result<TNodeId, &'static str> {
        let (tree, goal, ctx) = (self.tree, self.goal, self.ctx);
        let kind = view_kind(t, c);
        if kind == ViewKind::None {
            return Err("not a conversion");
        }
        if t.history.budget == 0 {
            return Err("implicit search too costly");
        }
        t.history.budget -= 1;
        let span = t.tpd.span(tree);
        let (fun, from) = (candidate_ref(t, c, span.start_pos(), ctx)?, t.constraint.count());
        let mut fun = t.with_type_vars(fun)?;
        if kind == ViewKind::Value {
            fun = t.viewing(c, |t| self.conversion_apply(t, fun))?;
        }
        let fun_tpe = t.tpd.tpe(fun);
        let info = t.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Some(Binding { params, result, implicit: false, .. }) = t.u.method(info) else {
            return Err("not a conversion");
        };
        let [(_, mut param)] = params[..] else { return Err("not a conversion") };
        if let Type::ByName(p) = *t.u.types().get(param) {
            param = p;
        }
        let arg_tpe = t.tpd.tpe(tree);
        let arg = if t.conforms(arg_tpe, param) {
            t.gadt_adapted(tree, param, ctx)
        } else if let Some(c) = t.captured_wildcards(arg_tpe).filter(|&c| t.conforms(c, param)) {
            t.cast_to_captured(tree, c)?
        } else if widen && t.widens_to(arg_tpe, param) {
            t.converted_constant(tree, param).ok_or("conversion of another type")?
        } else {
            return Err("conversion of another type");
        };
        let converted = t.build().apply(fun, &[arg], result, span.to_synthetic());
        if let Type::Method { params, implicit: true, .. } = t.u.types().get(result).clone() {
            if let Goal::Conform(pt) = goal {
                let value = t.strip_implicit(result);
                let (fits, _, _) = t.speculatively(|t| t.conforms(value, pt));
                if !fits {
                    return Err("conversion to another type");
                }
                t.u.is_necessary_subtype_in(value, pt, &mut t.constraint);
            }
            let formals: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
            let locked = std::mem::replace(&mut t.locked_vars, from);
            let instantiated = t.instantiate_param_vars(converted, &formals);
            t.locked_vars = locked;
            instantiated?;
        }
        let spans = (span.end_pos(), span.to_synthetic());
        let converted = t.viewing(c, |t| t.with_implicit_args(converted, spans, ctx, 1))?;
        let result = t.tpd.tpe(converted);
        let pt = match goal {
            Goal::Conform(pt) if t.conforms(result, pt) => Pt::Type(pt),
            Goal::Conform(_) => return Err("conversion to another type"),
            Goal::Member(_) | Goal::Call(..) | Goal::Typed(..) => Pt::Any,
        };
        t.interpolate(converted, pt, from)?;
        let result = t.zonk(result);
        t.tpd.get_mut(converted).tpe = Some(result);
        if let Some(name) = goal.member()
            && !t.type_members(result, name).iter().any(|&m| !t.u.symbol(m).is_private())
        {
            return Err("conversion to another type");
        }
        Ok(converted)
    }

    /// Applies conversion value `fun` to its implicit arguments and selects its `apply`, its type
    /// first constrained to take the tree to the goal (dotty's `typedImplicit` adapting it to
    /// `Conversion[from, pt]`).
    fn conversion_apply(&self, t: &mut Typer<'_>, fun: TNodeId) -> Result<TNodeId, &'static str> {
        let tpe = t.tpd.tpe(fun);
        let info = t.method_info(tpe).unwrap_or(tpe);
        let mut value = t.strip_implicit(info);
        if let Type::ByName(r) = *t.u.types().get(value) {
            value = r;
        }
        let to = match self.goal {
            Goal::Conform(pt) => pt,
            Goal::Member(_) | Goal::Call(..) | Goal::Typed(..) => t.u.intern_type(Type::Wildcard(None)),
        };
        let function = t.function_class(1).ok_or("Function1")?;
        let expected = t.u.intern_type(Type::Applied(function, vec![t.tpd.tpe(self.tree), to]));
        if !t.conforms(value, expected) {
            return Err("conversion of another type");
        }
        let at = t.tpd.span(self.tree).start_pos();
        let fun = t.with_implicit_args(fun, (at, at), self.ctx, 1)?;
        let fun = t.inlined_transparent(fun, Pt::Any, self.ctx).unwrap_or(fun);
        t.with_apply(fun, Pt::Any, self.ctx)
    }
}
