//! Implicit parameter clauses applied to the arguments the search finds (dotty's
//! `adaptNoArgsImplicitMethod`).

use super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{Attrs, NodeId, Span};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `tree` applied to the arguments of its leading implicit clauses, searched at `at` and
    /// applied at `span`.
    pub fn with_implicit_args(
        &mut self,
        mut tree: TNodeId,
        spans: (Span, Span),
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        while let Some(applied) = self.with_implicit_clause(tree, spans, ctx, depth)? {
            tree = applied;
        }
        Ok(tree)
    }

    /// `tree` applied to the arguments of its first parameter clause, searched at `at` and applied
    /// at `span`; `None` if that clause is not implicit.
    pub fn with_implicit_clause(
        &mut self,
        tree: TNodeId,
        (at, span): (Span, Span),
        ctx: Ctx,
        depth: u32,
    ) -> Result<Option<TNodeId>, &'static str> {
        let t = self.tpd.tpe(tree);
        let info = self.method_info(t).unwrap_or(t);
        let Type::Method { binder, params, result, implicit: true, .. } = self.u.types().get(info).clone()
        else {
            return Ok(None);
        };
        let mut args = Vec::new();
        for &(_, p) in &params {
            args.push(self.implicit_arg(p, at, ctx, depth)?);
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let arg_types: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let meth = self.tpd.sym(self.tpd.meth_part(tree));
        let result = self.applied_result(result, (binder, &names), meth, &arg_types);
        Ok(Some(self.build().apply(tree, &args, result, span)))
    }

    /// `tree`, a method reference whose first clause is implicit, applied to that clause's
    /// arguments searched in `ctx`, its type variables in them instantiated first.
    pub fn implicit_clause_applied(&mut self, tree: TNodeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(tree);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Method { params, implicit: true, .. } = self.u.types().get(info).clone() else {
            return Ok(tree);
        };
        let formals: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
        self.instantiate_param_vars(tree, &formals)?;
        let args_ctx = self.implicit_args_ctx(tree, ctx);
        let span = self.tpd.span(tree);
        let applied = self.with_implicit_clause(tree, (span.end_pos(), span.to_synthetic()), args_ctx, 0)?;
        Ok(applied.unwrap_or(tree))
    }

    /// Function `f` of application `id` applied to the arguments of its leading `using` clauses
    /// if `id` does not pass them (dotty's `adaptToArgs` without a `matchingApply`).
    pub fn with_using_args(&mut self, mut f: TNodeId, id: NodeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        while self.takes_using(f) && !self.ast.attrs(id).contains(Attrs::USING_APPLY) {
            f = self.implicit_clause_applied(f, ctx)?;
            f = self.with_type_vars(f)?;
        }
        Ok(f)
    }

    /// Whether the first parameter clause of function `f` is a `using` clause.
    pub fn takes_using(&mut self, f: TNodeId) -> bool {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        matches!(self.u.types().get(info), Type::Method { contextual: true, .. })
    }
}
