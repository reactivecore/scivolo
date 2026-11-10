// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala

mod defaults;
mod missing_given;

use super::super::Typer;
use super::super::dependent_params::{applied_result, mentions_clause_params};
use super::search::AMBIGUOUS;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Binder, Binding, SymbolId, Type, TypeId};

/// The application of one implicit parameter clause of a tree to the arguments the searches find
/// for it, as `summon[Ordering[Int]]` gets `(using Ordering.Int)` (dotty's `addImplicitArgs` in
/// `adaptNoArgsImplicitMethod`).
pub struct ImplicitClause {
    /// The tree whose first parameter clause is implicit.
    tree: TNodeId,
    /// The method the tree refers to, if any.
    meth: Option<SymbolId>,
    binder: Binder,
    /// The parameters of the clause, by name and type.
    params: Vec<(String, TypeId)>,
    names: Vec<String>,
    /// The type after the clause, referring to its parameters.
    result: TypeId,
    /// Where the arguments are searched.
    at: Span,
    /// Where the application is placed.
    span: Span,
    ctx: Ctx,
    /// How deep the searches nest in implicit arguments of implicit arguments.
    depth: u32,
}

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
}

impl ImplicitClause {
    /// Prepares the application of the first parameter clause of `tree`, searched at the first of
    /// `spans` and applied at the second; `None` if that clause is not implicit.
    fn of(t: &mut Typer<'_>, tree: TNodeId, (at, span): (Span, Span), ctx: Ctx, depth: u32) -> Option<Self> {
        let tpe = t.tpd.tpe(tree);
        let info = t.method_info(tpe).unwrap_or(tpe);
        let Type::Method { binder, params, result, implicit: true, .. } = t.u.types().get(info).clone()
        else {
            return None;
        };
        let names = params.iter().map(|(n, _)| n.clone()).collect();
        let meth = t.tpd.sym(t.tpd.meth_part(tree));
        Some(ImplicitClause { tree, meth, binder, params, names, result, at, span, ctx, depth })
    }

    /// Applies the tree to an argument per parameter: the one found, else its default, else an
    /// error tree for a `given` no definition of the program can supply; fails when an argument
    /// is missing or ambiguous in a way dotty would report otherwise.
    fn applied(self, t: &mut Typer<'_>) -> Result<TNodeId, &'static str> {
        let mut found: Vec<Option<TNodeId>> = Vec::new();
        let mut ambiguous = false;
        for (i, (name, p)) in self.params.iter().enumerate() {
            if ambiguous && self.mentions_params(t, *p) {
                break;
            }
            if found.contains(&None) && self.mentions_params(t, *p) {
                return Err("implicit parameter depending on a defaulted one");
            }
            let before: Vec<TypeId> = found.iter().flatten().map(|&a| t.tpd.tpe(a)).collect();
            let formal = self.substituted(t, *p, &before);
            match t.implicit_arg(formal, self.at, self.ctx, self.depth) {
                Err("no implicit argument") if self.defaultable(t, i, formal) => found.push(None),
                Err("no implicit argument")
                    if self.depth > 0 && self.meth.is_some_and(|m| t.may_default(self.tree, m, i)) =>
                {
                    return Err("nested implicit argument left to its default");
                }
                Err(what @ "no implicit argument") if self.depth == 0 => {
                    return self.unsupplied(t, name, formal).ok_or(what);
                }
                Err(AMBIGUOUS) if self.depth > 0 => ambiguous = true,
                arg => found.push(Some(arg?)),
            }
        }
        if ambiguous {
            return Err(AMBIGUOUS);
        }
        if found.contains(&None) {
            return Ok(self.with_defaults(t, &found));
        }
        let args: Vec<TNodeId> = found.into_iter().flatten().collect();
        let arg_types: Vec<TypeId> = args.iter().map(|&a| t.tpd.tpe(a)).collect();
        let result = self.result;
        if t.types().without_false_deps_in(result, (self.binder, &self.params), self.meth) != result {
            return Err("implicit method type with a false dependency");
        }
        let result = self.substituted(t, result, &arg_types);
        Ok(t.build().apply(self.tree, &args, result, self.span))
    }

    /// Whether `tpe` refers to a parameter of the clause.
    fn mentions_params(&self, t: &mut Typer<'_>, tpe: TypeId) -> bool {
        mentions_clause_params(t.u, tpe, (self.binder, &self.names), self.meth)
    }

    /// `tpe` with the argument types `args` substituted for the first parameters of the clause.
    fn substituted(&self, t: &mut Typer<'_>, tpe: TypeId, args: &[TypeId]) -> TypeId {
        applied_result(t.u, tpe, (self.binder, &self.names), self.meth, args)
    }
}
