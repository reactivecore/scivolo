// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala

use super::super::Typer;
use super::super::dependent_params::{applied_result, mentions_clause_params};
use super::search::AMBIGUOUS;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{Binder, SymbolId, Type, TypeId};

/// The application of one implicit parameter clause of a tree to the arguments the searches find
/// for it, as `summon[Ordering[Int]]` gets `(using Ordering.Int)` (dotty's `addImplicitArgs`).
pub struct ImplicitClause {
    /// The tree whose first parameter clause is implicit.
    pub tree: TNodeId,
    /// The method the tree refers to, if any.
    pub meth: Option<SymbolId>,
    pub binder: Binder,
    /// The parameters of the clause, by name and type.
    pub params: Vec<(String, TypeId)>,
    pub names: Vec<String>,
    /// The type after the clause, referring to its parameters.
    pub result: TypeId,
    /// Where the arguments are searched.
    pub at: Span,
    /// Where the application is placed.
    pub span: Span,
    pub ctx: Ctx,
    /// How deep the searches nest in implicit arguments of implicit arguments.
    pub depth: u32,
}

impl ImplicitClause {
    /// Prepares the application of the first parameter clause of `tree`, searched at the first of
    /// `spans` and applied at the second; `None` if that clause is not implicit.
    pub fn of(
        t: &mut Typer<'_>,
        tree: TNodeId,
        (at, span): (Span, Span),
        ctx: Ctx,
        depth: u32,
    ) -> Option<Self> {
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
    pub fn applied(self, t: &mut Typer<'_>) -> Result<TNodeId, &'static str> {
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
