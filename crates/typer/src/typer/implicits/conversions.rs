//! Implicit conversions (dotty's `inferView`, `tryInsertImplicitOnQualifier`): an implicit method
//! of one parameter applied to a tree, to fit an expected type or to give it a member.

use super::super::{Pt, Typer};
use super::candidates::Candidate;
use super::view_kinds::ViewKind;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::tags::PRIVATE;
use tasty::{Type, TypeId};

/// What a conversion's result must offer.
#[derive(Clone, Copy)]
pub enum Goal<'a> {
    Conform(TypeId),
    Member(&'a str),
}

impl Typer<'_> {
    /// `tree` converted to fit `goal` by the implicit conversion the search selects, `None` if
    /// none applies.
    pub fn converted(
        &mut self,
        tree: TNodeId,
        goal: Goal,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let from = self.tpd.tpe(tree);
        let from = self.zonk(from);
        if let Goal::Conform(pt) = goal
            && (self.is_top_like(pt) || self.is_underspecified_arg(from))
        {
            return Ok(None);
        }
        self.implicit_budget = super::search::MAX_TRIES;
        let mut groups = self.contextual_implicits(ctx)?;
        let mut scope = self.implicit_scope(from);
        if let Goal::Conform(pt) = goal {
            scope.extend(self.implicit_scope(pt));
        }
        if let Goal::Member(name) = goal
            && scope.iter().any(|c| {
                let s = self.u.symbol(c.sym);
                s.name == name && s.flags.has(tasty::tags::EXTENSION)
            })
        {
            return Err("extension method in the implicit scope");
        }
        groups.push(scope);
        let constant = self.constant_of(tree).is_some();
        for group in groups {
            let found = self.fitting(&group, |t, c| t.try_conversion(c, (tree, false), goal, ctx))?;
            let mut widened = Vec::new();
            if constant {
                widened = self.fitting(&group, |t, c| t.try_conversion(c, (tree, true), goal, ctx))?;
                widened.retain(|c| !found.contains(c));
            }
            let ambiguous = "ambiguous implicit conversions";
            let winner = match (&found[..], &widened[..]) {
                ([], []) => continue,
                ([c], _) | ([], [c]) => *c,
                ([], _) => self.best_widened(&widened).ok_or(ambiguous)?,
                _ => self.most_specific(&found).ok_or(ambiguous)?,
            };
            if found.contains(&winner) && !self.beats_widened(winner, &widened) {
                return Err(ambiguous);
            }
            return self.try_conversion(winner, (tree, widened.contains(&winner)), goal, ctx).map(Some);
        }
        Ok(None)
    }

    /// Whether `pt` is `Any`, `AnyVal`, `AnyRef`, `Matchable` or `Unit`, which no conversion
    /// targets.
    fn is_top_like(&mut self, pt: TypeId) -> bool {
        let pt = self.zonk(pt);
        let b = self.u.builtins();
        let class = self.u.class_symbol(pt);
        class.is_some_and(|c| {
            c == b.any
                || c == b.object
                || c == b.matchable
                || Some(c) == b.any_val
                || self.u.full_name(c) == "scala.Unit"
        })
    }

    /// Whether a value of type `from` is `null` or has no values, so no conversion takes it
    /// (dotty's `isUnderSpecifiedArgument`).
    fn is_underspecified_arg(&mut self, from: TypeId) -> bool {
        let widened = self.u.widen(from);
        let b = self.u.builtins();
        self.u.class_symbol(widened).is_some_and(|c| c == b.nothing || c == b.null)
    }

    /// `c(tree)` with `c`'s implicit arguments, if it fits `goal`, constant `tree` converted to
    /// the parameter type if `widen`.
    fn try_conversion(
        &mut self,
        c: Candidate,
        (tree, widen): (TNodeId, bool),
        goal: Goal,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let kind = self.view_kind(c);
        if let Goal::Member(name) = goal
            && self.offers_extension(c, name)
        {
            return Err("extension method of an implicit value");
        }
        if kind == ViewKind::None {
            return Err("not a conversion");
        }
        if self.implicit_budget == 0 {
            return Err("implicit search too costly");
        }
        self.implicit_budget -= 1;
        let span = self.tpd.span(tree);
        let fun = self.candidate_ref(c, span.start_pos(), ctx)?;
        let from = self.constraint.count();
        let mut fun = self.with_type_vars(fun)?;
        if kind == ViewKind::Value {
            fun = self.conversion_apply(fun, tree, goal, ctx)?;
        }
        let fun_tpe = self.tpd.tpe(fun);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Method { params, result, implicit: false, .. } = self.u.types().get(info).clone() else {
            return Err("not a conversion");
        };
        let [(_, mut param)] = params[..] else { return Err("not a conversion") };
        if let Type::ByName(p) = *self.u.types().get(param) {
            param = p;
        }
        let arg_tpe = self.tpd.tpe(tree);
        let arg = if self.conforms(arg_tpe, param) {
            tree
        } else if widen && self.widens_to(arg_tpe, param) {
            self.converted_constant(tree, param).ok_or("conversion of another type")?
        } else {
            return Err("conversion of another type");
        };
        let converted = self.build().apply(fun, &[arg], result, span.to_synthetic());
        let converted = self.with_implicit_args(converted, (span.end_pos(), span.to_synthetic()), ctx, 1)?;
        let result = self.tpd.tpe(converted);
        let pt = match goal {
            Goal::Conform(pt) if self.conforms(result, pt) => Pt::Type(pt),
            Goal::Conform(_) => return Err("conversion to another type"),
            Goal::Member(_) => Pt::Any,
        };
        self.interpolate(converted, pt, from)?;
        let result = self.zonk(result);
        self.tpd.get_mut(converted).tpe = Some(result);
        if let Goal::Member(name) = goal
            && !self.type_members(result, name).iter().any(|&m| !self.u.symbol(m).flags.has(PRIVATE))
        {
            return Err("conversion to another type");
        }
        Ok(converted)
    }

    /// Conversion value `fun` applied to its implicit arguments and its `apply` selected, its type
    /// first constrained to take `tree` to `goal` (dotty's `typedImplicit` adapting it to
    /// `Conversion[from, pt]`).
    fn conversion_apply(
        &mut self,
        fun: TNodeId,
        tree: TNodeId,
        goal: Goal,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let t = self.tpd.tpe(fun);
        let info = self.method_info(t).unwrap_or(t);
        let mut value = self.strip_implicit(info);
        if let Type::ByName(r) = *self.u.types().get(value) {
            value = r;
        }
        let to = match goal {
            Goal::Conform(pt) => pt,
            Goal::Member(_) => self.u.intern_type(Type::Wildcard(None)),
        };
        let function = self.function_class(1).ok_or("Function1")?;
        let expected = self.u.intern_type(Type::Applied(function, vec![self.tpd.tpe(tree), to]));
        if !self.conforms(value, expected) {
            return Err("conversion of another type");
        }
        let at = self.tpd.span(tree).start_pos();
        let fun = self.with_implicit_args(fun, (at, at), ctx, 1)?;
        self.with_apply(fun, Pt::Any, ctx)
    }
}
