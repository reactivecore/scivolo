// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Implicit conversions (dotty's `inferView`, `tryInsertImplicitOnQualifier`): an implicit method
//! of one parameter applied to a tree, to fit an expected type or to give it a member.

use super::super::{Pt, Typer};
use super::candidates::Candidate;
use super::view_kinds::ViewKind;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::tags::PRIVATE;
use tasty::{Binding, Type, TypeId};

/// What a conversion's result must offer.
#[derive(Clone, Copy)]
pub enum Goal<'a> {
    Conform(TypeId),
    Member(&'a str),
}

/// What a view search found: a converted tree, or an implicit value with the extension method a
/// selection names.
pub enum Viewed {
    Converted(TNodeId),
    Extension(Candidate),
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
        match self.viewed(tree, goal, ctx)? {
            Some(Viewed::Converted(t)) => Ok(Some(t)),
            Some(Viewed::Extension(_)) => Err("extension method of an implicit value"),
            None => Ok(None),
        }
    }

    /// `tree` converted to fit `goal` by the implicit conversion the search selects, or the
    /// implicit value with the extension method a member goal names; `None` if neither applies.
    pub fn viewed(&mut self, tree: TNodeId, goal: Goal, ctx: Ctx) -> Result<Option<Viewed>, &'static str> {
        let from = self.zonk(self.tpd.tpe(tree));
        if let Goal::Conform(pt) = goal
            && (self.is_top_like(pt) || self.is_underspecified_arg(from))
        {
            return Ok(None);
        }
        if self.history.open.is_empty() {
            self.implicit_budget = super::search::MAX_TRIES;
        }
        let contextual = self.contextual_implicits(ctx, &mut |t, c| {
            t.view_kind(c) != ViewKind::None
                || matches!(goal, Goal::Member(name) if t.offers_extension(c, name).unwrap_or(false))
        })?;
        let levels = contextual.len();
        let mut contextual = contextual.into_iter();
        let constant = self.constant_of_type(from).is_some();
        for _ in 0..=levels {
            let (mut group, containers) = match contextual.next() {
                Some(group) => (group, Vec::new()),
                None => self.view_scope(from, goal)?,
            };
            let found = self.fitting(&group, |t, c| t.try_conversion(c, (tree, false), goal, ctx))?;
            let mut widened = Vec::new();
            if constant {
                widened = self.fitting(&group, |t, c| t.try_conversion(c, (tree, true), goal, ctx))?;
                widened.retain(|c| !found.contains(c));
            }
            if let Goal::Member(name) = goal {
                let extra: Vec<Candidate> = containers.into_iter().filter(|c| !group.contains(c)).collect();
                group.extend(extra);
                let converts = !found.is_empty() || !widened.is_empty();
                if let Some(c) = self.extension_container(&group, tree, name, converts, ctx)? {
                    return Ok(Some(Viewed::Extension(c)));
                }
            }
            let ambiguous = "ambiguous implicit conversions";
            let winner = match (&found[..], &widened[..]) {
                ([], []) => continue,
                ([c], _) | ([], [c]) => *c,
                ([], _) => self.best_widened(&widened).ok_or(ambiguous)?,
                _ => self.most_specific(&found, ctx).ok_or(ambiguous)?,
            };
            if found.contains(&winner) && !self.beats_widened(winner, &widened) {
                return Err(ambiguous);
            }
            let converted = self.try_conversion(winner, (tree, widened.contains(&winner)), goal, ctx)?;
            return Ok(Some(Viewed::Converted(converted)));
        }
        Ok(None)
    }

    /// `c(tree)` with `c`'s implicit arguments, if it fits `goal`, constant `tree` converted to
    /// the parameter type if `widen`, wildcards in its type captured if that fits.
    pub fn try_conversion(
        &mut self,
        c: Candidate,
        (tree, widen): (TNodeId, bool),
        goal: Goal,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let kind = self.view_kind(c);
        if kind == ViewKind::None {
            return Err("not a conversion");
        }
        if self.implicit_budget == 0 {
            return Err("implicit search too costly");
        }
        self.implicit_budget -= 1;
        let span = self.tpd.span(tree);
        let (fun, from) = (self.candidate_ref(c, span.start_pos(), ctx)?, self.constraint.count());
        let mut fun = self.with_type_vars(fun)?;
        if kind == ViewKind::Value {
            fun = self.viewing(c, |t| t.conversion_apply(fun, tree, goal, ctx))?;
        }
        let fun_tpe = self.tpd.tpe(fun);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Some(Binding { params, result, implicit: false, .. }) = self.u.method(info) else {
            return Err("not a conversion");
        };
        let [(_, mut param)] = params[..] else { return Err("not a conversion") };
        if let Type::ByName(p) = *self.u.types().get(param) {
            param = p;
        }
        let arg_tpe = self.tpd.tpe(tree);
        let arg = if self.conforms(arg_tpe, param) {
            tree
        } else if let Some(c) = self.captured_wildcards(arg_tpe).filter(|&c| self.conforms(c, param)) {
            self.cast_to_captured(tree, c)?
        } else if widen && self.widens_to(arg_tpe, param) {
            self.converted_constant(tree, param).ok_or("conversion of another type")?
        } else {
            return Err("conversion of another type");
        };
        let converted = self.build().apply(fun, &[arg], result, span.to_synthetic());
        if let Type::Method { params, implicit: true, .. } = self.u.types().get(result).clone() {
            if let Goal::Conform(pt) = goal {
                let value = self.strip_implicit(result);
                let (fits, _, _) = self.speculatively(|t| t.conforms(value, pt));
                if !fits {
                    return Err("conversion to another type");
                }
                self.u.is_necessary_subtype_in(value, pt, &mut self.constraint);
            }
            let formals: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
            let locked = std::mem::replace(&mut self.locked_vars, from);
            let instantiated = self.instantiate_param_vars(converted, &formals);
            self.locked_vars = locked;
            instantiated?;
        }
        let spans = (span.end_pos(), span.to_synthetic());
        let converted = self.viewing(c, |t| t.with_implicit_args(converted, spans, ctx, 1))?;
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
}
