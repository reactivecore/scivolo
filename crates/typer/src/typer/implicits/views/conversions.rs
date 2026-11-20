// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Applying one conversion candidate to the tree of a view search (dotty's `typedImplicit` of a
//! view).

use super::super::super::widening::widens;
use super::super::super::{Pt, Typer};
use super::super::candidates::{Candidate, INHERITED_THROUGH_PACKAGE, candidate_ref};
use super::view_kinds::{ViewKind, view_kind};
use super::{Goal, ViewSearch};
use crate::tpd::TNodeId;
use tasty::{Binding, Type, TypeId};

/// The class of given conversions.
const CONVERSION: &str = "scala.Conversion";

impl ViewSearch<'_> {
    /// Applies conversion `c` to the tree with `c`'s implicit arguments if the result fits the
    /// goal, as `1` becomes `intToBig(1)`: a constant tree widened to the parameter type first if
    /// `widen`, the wildcards of its type captured where that fits, before trying it uncaptured
    /// unless the parameter is by-name (dotty's `captureWildcardsCompat` before the view search
    /// in `adapt`, which a by-name parameter fails by `checkNoWildcardCaptureForCBN`); the result
    /// constrained by the goal first (dotty's `constrainResult`).
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
        let fun = match candidate_ref(t, c, span.start_pos(), ctx) {
            Err(INHERITED_THROUGH_PACKAGE) if never_applies(t, c, t.tpd.tpe(tree), goal) => {
                return Err("conversion of another type");
            }
            fun => fun?,
        };
        let from = t.constraint.count();
        let mut fun = t.with_type_vars(fun)?;
        if kind == ViewKind::Value {
            fun = t.viewing(c, goal, |t| self.conversion_apply(t, fun, from))?;
        }
        let fun_tpe = t.tpd.tpe(fun);
        let info = t.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Some(Binding { params, result, implicit: false, .. }) = t.u.method(info) else {
            return Err("not a conversion");
        };
        let [(_, param)] = params[..] else { return Err("not a conversion") };
        let (param, by_name) = t.u.by_name(param).map_or((param, false), |p| (p, true));
        if let Goal::Conform(pt) = goal
            && t.u.method(result).is_none()
            && t.speculatively(|t| t.conforms(result, pt)).0
        {
            t.u.is_necessary_subtype_in(result, pt, &mut t.constraint);
        }
        let arg_tpe = t.tpd.tpe(tree);
        let arg = if by_name && t.conforms(arg_tpe, param) {
            t.gadt_adapted(tree, param)
        } else if let Some(c) = t.captured_wildcards(arg_tpe).filter(|&c| t.conforms(c, param)) {
            t.cast_to_captured(tree, c)?
        } else if t.conforms(arg_tpe, param) {
            t.gadt_adapted(tree, param)
        } else if widen && t.types().widens_to(arg_tpe, param) {
            t.converted_constant(tree, param).ok_or("conversion of another type")?
        } else {
            return Err("conversion of another type");
        };
        let converted = t.build().apply(fun, &[arg], result, span.to_synthetic());
        if let Some(params) = t.u.method(result).filter(|m| m.implicit).map(|m| m.params.to_vec()) {
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
        let converted = t.viewing(c, goal, |t| t.with_implicit_args(converted, spans, ctx, 1))?;
        let result = t.tpd.tpe(converted);
        let pt = match goal {
            Goal::Conform(pt) if t.conforms(result, pt) => Pt::Type(pt),
            Goal::Conform(_) => return Err("conversion to another type"),
            _ => Pt::Any,
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
    /// first constrained to take the tree to the goal and the type variables of id `from` or later
    /// interpolated against `Conversion[A, pt]`, `A` the tree's type (dotty's `typedImplicit`
    /// adapting it to that type).
    fn conversion_apply(
        &self,
        t: &mut Typer<'_>,
        fun: TNodeId,
        from: usize,
    ) -> Result<TNodeId, &'static str> {
        let tpe = t.tpd.tpe(fun);
        let info = t.method_info(tpe).unwrap_or(tpe);
        let mut value = t.strip_implicit(info);
        if let Some(r) = t.u.by_name(value) {
            value = r;
        }
        let to = match self.goal {
            Goal::Conform(pt) => pt,
            _ => t.u.intern_type(Type::Wildcard(None)),
        };
        let function = t.function_class(1).ok_or("Function1")?;
        let expected = t.u.intern_applied(function, &[t.tpd.tpe(self.tree), to]);
        if !t.conforms(value, expected) && !t.defined_further(value, expected) {
            return Err("conversion of another type");
        }
        let at = t.tpd.span(self.tree).start_pos();
        let fun = t.with_implicit_args(fun, (at, at), self.ctx, 1)?;
        let conversion = t.u.class_ref(CONVERSION);
        let adapted_to = t.u.intern_applied(conversion, &[t.tpd.tpe(self.tree), to]);
        t.interpolate(fun, Pt::Type(adapted_to), from)?;
        t.settle_type(fun, self.ctx);
        let fun = t.inlined_transparent(fun, Pt::Any, self.ctx).unwrap_or(fun);
        let fun = match self.goal {
            Goal::Conform(pt) => self.gadt_cast_conversion(t, fun, value, pt),
            _ => fun,
        };
        t.with_apply(fun, Pt::Any, self.ctx)
    }

    /// Conversion value `fun` of type `value` cast to `Conversion[from, pt]` if it is one only by
    /// GADT bounds; a goal other than a type is a prototype, never a cast's target (dotty's
    /// `tryConversion` adapting a given `Conversion` to `pt`, and `insertGadtCast`).
    fn gadt_cast_conversion(&self, t: &mut Typer<'_>, fun: TNodeId, value: TypeId, pt: TypeId) -> TNodeId {
        let conversion = t.u.class_ref(CONVERSION);
        let derives = match (t.u.class_symbol(value), t.u.class_symbol(conversion)) {
            (Some(c), Some(conv)) => t.u.derives_from(c, conv),
            _ => false,
        };
        if !derives {
            return fun;
        }
        let target = t.u.intern_applied(conversion, &[t.tpd.tpe(self.tree), pt]);
        t.gadt_adapted(fun, target)
    }
}

/// Whether conversion method `c` cannot convert a tree of type `arg` to fit `goal`: its result
/// cannot (`may_view`), or its parameter is of a class that the class of `arg`, neither `Null`
/// nor `Nothing`, does not derive from nor widen to, as `Seq[T]` and `Array[String]`.
fn never_applies(t: &mut Typer<'_>, c: Candidate, arg: TypeId, goal: Goal) -> bool {
    let info = t.u.info(c.sym);
    let info = t.u.poly(info).map_or(info, |p| p.result);
    let Some(Binding { params, .. }) = t.u.method(info) else { return false };
    let [(_, param)] = params[..] else { return false };
    let param = t.u.by_name(param).unwrap_or(param);
    let arg = t.u.widen(arg);
    let (Some(p), Some(a)) = (t.u.class_symbol(param), t.u.class_symbol(arg)) else { return false };
    let builtins = t.u.defn();
    let bottom = a == builtins.nothing || a == builtins.null;
    let widened = widens(&t.u.full_name(a), &t.u.full_name(p));
    !t.may_view(c.sym, a, goal) || !bottom && !widened && !t.u.derives_from(a, p)
}
