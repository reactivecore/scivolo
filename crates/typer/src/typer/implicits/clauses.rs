// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Implicit parameter clauses applied to the arguments the search finds (dotty's
//! `adaptNoArgsImplicitMethod`).

use super::super::{Pt, Typer};
use super::implicit_clause::ImplicitClause;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Binding, Type, TypeId};

impl Typer<'_> {
    /// Applies `tree` to the arguments of its leading implicit clauses, searched at the first of
    /// `spans` and applied at the second, as `f` becomes `f(using a)(using b)`.
    pub fn with_implicit_args(
        &mut self,
        mut tree: TNodeId,
        spans: (Span, Span),
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        while let Some(clause) = ImplicitClause::of(self, tree, spans, ctx, depth) {
            tree = clause.applied(self)?;
        }
        Ok(tree)
    }

    /// Applies `tree`, a method reference such as `summon[Ordering[Int]]`, to the arguments of its
    /// first clause if that is implicit, searched in `ctx` after the type variables of its
    /// parameter types are instantiated; a block's definitions stay around the application.
    pub fn implicit_clause_applied(&mut self, tree: TNodeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        if let T::Block { stats, expr } = *self.tpd.tree(tree) {
            let mut stats = self.tpd.items(stats).to_vec();
            let mut applied = self.implicit_clause_applied(expr, ctx)?;
            if let T::Block { stats: inner, expr } = *self.tpd.tree(applied) {
                stats.extend_from_slice(self.tpd.items(inner));
                applied = expr;
            }
            if applied != expr {
                self.tpd.get_mut(applied).span = self.tpd.span(applied).to_synthetic();
            }
            return Ok(self.wrap_defs(stats, applied));
        }
        let tpe = self.tpd.tpe(tree);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Some(Binding { params, implicit: true, .. }) = self.u.method(info) else {
            return Ok(tree);
        };
        let formals: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
        self.instantiate_param_vars(tree, &formals)?;
        let args_ctx = self.implicit_args_ctx(tree, ctx);
        let span = self.tpd.span(tree);
        match ImplicitClause::of(self, tree, (span.end_pos(), span.to_synthetic()), args_ctx, 0) {
            Some(clause) => clause.applied(self),
            None => Ok(tree),
        }
    }

    /// Interpolates the type variables of id `from` or later again and zonks the type if
    /// `adapted`, the adaptation of `tree` to `pt`, applies it (dotty's `readaptSimplified`).
    pub fn reinterpolate(
        &mut self,
        (tree, adapted): (TNodeId, TNodeId),
        pt: Pt,
        from: usize,
    ) -> Result<(), &'static str> {
        match *self.tpd.tree(adapted) {
            T::Apply { fun, .. }
                if adapted != tree && self.tpd.meth_part(fun) == self.tpd.meth_part(tree) =>
            {
                self.interpolate_zonked(adapted, pt, from)
            }
            _ => Ok(()),
        }
    }

    /// Applies function `f` to the arguments of its leading `using` clauses, unless the call
    /// passes them itself as in `f(using x)`, flagged by `using` (dotty's `adaptToArgs` without a
    /// `matchingApply`).
    pub fn with_using_args(
        &mut self,
        mut f: TNodeId,
        using: bool,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        while !using && self.takes_using(f) {
            f = self.implicit_clause_applied(f, ctx)?;
            f = self.with_type_vars(f)?;
        }
        Ok(f)
    }

    /// Partial application `tree` of a call whose arguments were lifted into `defs`; `Err` unless
    /// applied further or only implicit or `using` clauses follow, without open type variables.
    pub fn lifted_partial(
        &mut self,
        tree: TNodeId,
        defs: &[TNodeId],
        pt: Pt,
    ) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(tree);
        if defs.is_empty() || !matches!(self.u.types().get(tpe), Type::Method { .. }) {
            return Ok(tree);
        }
        if matches!(pt, Pt::Fun(_)) {
            return Ok(tree);
        }
        let implicit = matches!(self.u.types().get(tpe), Type::Method { implicit: true, .. });
        if matches!(pt, Pt::PolyFun(_)) || !(implicit || self.takes_using(tree)) || self.is_open(tpe) {
            return Err("lifted call of a curried method");
        }
        Ok(tree)
    }

    /// The definitions and the method of `f`, a partial application after its lifted arguments
    /// as `lifted_partial` leaves it, or none and `f`.
    pub fn lifted_fun(&mut self, f: TNodeId) -> (Vec<TNodeId>, TNodeId) {
        match *self.tpd.tree(f) {
            T::Block { stats, expr }
                if matches!(self.u.types().get(self.tpd.tpe(expr)), Type::Method { .. }) =>
            {
                (self.tpd.items(stats).to_vec(), expr)
            }
            _ => (Vec::new(), f),
        }
    }

    /// Whether the first parameter clause of function `f` is a `using` clause.
    pub fn takes_using(&mut self, f: TNodeId) -> bool {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        matches!(self.u.types().get(info), Type::Method { contextual: true, .. })
    }
}
