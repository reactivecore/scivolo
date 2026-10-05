//! Implicit conversions (dotty's `inferView`, `tryInsertImplicitOnQualifier`): an implicit method
//! of one parameter applied to a tree, to fit an expected type or to give it a member.

use super::super::Typer;
use super::candidates::Candidate;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::GIVEN;
use tasty::{Kind, Type, TypeId};

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
            && self.is_top_like(pt)
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
        for group in groups {
            for &c in &group {
                if self.u.symbol(c.sym).flags.has(GIVEN) {
                    return Err("given conversion");
                }
                if self.u.symbol(c.sym).kind == Kind::Val && self.is_function_value(c) {
                    return Err("implicit function value");
                }
            }
            let found = self.fitting(&group, |t, c| t.try_conversion(c, tree, goal, ctx))?;
            let winner = match found.as_slice() {
                [] => continue,
                [c] => *c,
                _ => self.most_specific(&found).ok_or("ambiguous implicit conversions")?,
            };
            return self.try_conversion(winner, tree, goal, ctx).map(Some);
        }
        Ok(None)
    }

    /// Whether candidate value `c` is a function or a `Conversion`, which dotty applies as a view.
    fn is_function_value(&mut self, c: Candidate) -> bool {
        self.complete(c.sym);
        let tpe = self.ref_type(c.prefix, c.sym);
        let widened = self.u.widen(tpe);
        self.u.class_symbol(widened).is_some_and(|k| {
            let base = |t: &mut Self, name: &str| {
                t.resolve_full_name(name, true).is_some_and(|b| t.u.derives_from(k, b))
            };
            base(self, "scala.Function1") || base(self, "scala.Conversion")
        })
    }

    /// Whether `pt` is `Any`, `AnyVal`, `AnyRef` or `Matchable`, which no conversion targets.
    fn is_top_like(&mut self, pt: TypeId) -> bool {
        let pt = self.zonk(pt);
        let b = self.u.builtins();
        let class = self.u.class_symbol(pt);
        class.is_some_and(|c| c == b.any || c == b.object || c == b.matchable || Some(c) == b.any_val)
    }

    /// `c(tree)` with `c`'s implicit arguments, if it fits `goal`.
    fn try_conversion(
        &mut self,
        c: Candidate,
        tree: TNodeId,
        goal: Goal,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        if self.implicit_budget == 0 {
            return Err("implicit search too costly");
        }
        self.implicit_budget -= 1;
        if self.u.symbol(c.sym).kind != Kind::Def {
            return Err("not a conversion");
        }
        self.complete(c.sym);
        let span = self.tpd.span(tree);
        let name = self.u.symbol(c.sym).name.clone();
        let name = TName { name: self.name(&name), is_type: false };
        let tpe = self.ref_type(c.prefix, c.sym);
        let fun = self.tpd.node(T::Ident { name }, span.start_pos(), Some(tpe), Some(c.sym));
        let fun = self.with_type_vars(fun)?;
        let fun_tpe = self.tpd.tpe(fun);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Method { params, result, implicit: false, .. } = self.u.types().get(info).clone() else {
            return Err("not a conversion");
        };
        let [(_, param)] = params[..] else { return Err("not a conversion") };
        let arg_tpe = self.tpd.tpe(tree);
        if !self.conforms(arg_tpe, param) {
            return Err("conversion of another type");
        }
        let args = self.tpd.list(&[tree]);
        let mut converted = self.tpd.node(T::Apply { fun, args }, span.to_synthetic(), Some(result), None);
        while let Type::Method { params, result, implicit: true, .. } =
            self.u.types().get(self.tpd.tpe(converted)).clone()
        {
            let mut args = Vec::new();
            for (_, p) in params {
                args.push(self.implicit_arg(p, span.end_pos(), ctx, 1)?);
            }
            let args = self.tpd.list(&args);
            converted =
                self.tpd.node(T::Apply { fun: converted, args }, span.to_synthetic(), Some(result), None);
        }
        let result = self.tpd.tpe(converted);
        let fits = match goal {
            Goal::Conform(pt) => self.conforms(result, pt),
            Goal::Member(name) => {
                let result = self.zonk(result);
                let members = self.type_members(result, name);
                members.iter().any(|&m| !self.u.symbol(m).flags.has(tasty::tags::PRIVATE))
            }
        };
        if fits { Ok(converted) } else { Err("conversion to another type") }
    }
}
