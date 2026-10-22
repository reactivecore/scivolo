// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Context bounds of polymorphic functions as evidence parameters (dotty's `elimContextBounds`
//! of a `PolyFunction`, `addEvidenceParams` of a `PolyFunctionApply`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{Attrs, List, NodeId, Span, TName, Tree};
use tasty::{Kind, Type, TypeId};

/// An evidence parameter: its name and its context bound.
type Evidence = (TName, NodeId);

/// Where the evidence of a polymorphic function goes (dotty's `fitEvidenceParams`).
pub enum Fit {
    /// A parameter refers to the evidence: a clause before the parameters.
    Before,
    /// The parameters are given ones: joined in front of them.
    Joined,
    /// A context function around the result.
    After,
}

impl Typer<'_> {
    /// Type parameters `targs` without their context bounds, and the evidence of those bounds,
    /// named by the bound or `evidence$N` (dotty's `desugarContextBounds`).
    pub fn poly_evidence(&mut self, targs: List) -> (List, Vec<Evidence>) {
        let (mut evidence, mut seen) = (Vec::new(), 0);
        let items = self.ast.items(targs).to_vec();
        let mut mapped = Vec::new();
        for &t in &items {
            let Tree::TypeDef { name, rhs, mods } = *self.ast.tree(t) else {
                mapped.push(t);
                continue;
            };
            let Tree::ContextBounds { bounds, cx_bounds } = *self.ast.tree(rhs) else {
                mapped.push(t);
                continue;
            };
            for bound in self.ast.items(cx_bounds).to_vec() {
                let name = match *self.ast.tree(bound) {
                    Tree::ContextBoundTypeTree { own_name, .. } if !self.text(own_name.name).is_empty() => {
                        own_name
                    }
                    _ => {
                        seen += 1;
                        TName { name: self.name(&format!("evidence${seen}")), is_type: false }
                    }
                };
                evidence.push((name, bound));
            }
            mapped.push(self.ast.derived(t, Tree::TypeDef { name, rhs: bounds, mods }));
        }
        let targs = if mapped == items { targs } else { self.ast.list(&mapped) };
        (targs, evidence)
    }

    /// Where `evidence` goes among parameters `vparams` (dotty's `fitEvidenceParams`).
    pub fn evidence_fit(&self, evidence: &[Evidence], vparams: List) -> Fit {
        let names: Vec<TName> = evidence.iter().map(|&(n, _)| n).collect();
        let params = self.ast.items(vparams);
        let refers = params.iter().any(|&p| {
            let Tree::ValDef { tpt, .. } = *self.ast.tree(p) else { return false };
            self.ast.exists_subtree(
                tpt,
                &|ast, n| matches!(*ast.tree(n), Tree::Ident { name } if !name.is_type && names.contains(&name)),
            )
        });
        let given = |p: NodeId| {
            self.ast.mods_of(p).is_some_and(|m| self.ast.modifiers(m).is_one_of(f::GIVEN_OR_IMPLICIT))
        };
        if refers {
            Fit::Before
        } else if params.first().is_some_and(|&p| given(p)) {
            Fit::Joined
        } else {
            Fit::After
        }
    }

    /// Result type tree `result` as the context function type tree over `evidence`, refined by
    /// its `apply` (dotty's `functionsOf` for the result of a `PolyFunctionApply`, typed by
    /// `typedDependent`); `result` itself without evidence.
    pub fn evidence_function_tree(
        &mut self,
        evidence: &[Evidence],
        result: TNodeId,
        ctx: Ctx,
    ) -> Result<TNodeId, TNodeId> {
        let Some(&(_, first)) = evidence.first() else { return Ok(result) };
        let Some(class) = self.context_function_class(evidence.len()) else {
            return Err(self.unsupported(first, "context function type of that many parameters"));
        };
        let refinement = self.refinement_class(ctx);
        let apply = self.syms.define(self.u, "apply", Kind::Def, refinement, f::METHOD);
        let (mut params, mut param_trees, mut args) = (Vec::new(), Vec::new(), Vec::new());
        for &(name, bound) in evidence {
            let typed = self.typed_type(bound, ctx);
            if self.is_error(typed) {
                return Err(typed);
            }
            let (text, tpe) = (self.text(name.name).to_owned(), self.tpd.tpe(typed));
            let flags = f::GIVEN.or(f::TERM_PARAM).to_term_flags();
            let p = self.syms.define(self.u, &text, Kind::Param, apply, flags);
            self.u.set_info(p, tpe);
            let span = self.ast.span(bound).to_synthetic();
            param_trees.push(self.build().val_def(p, typed, None, span));
            args.push(self.type_tree(tpe, self.tpd.span(typed).to_synthetic(), false));
            params.push((text, tpe));
        }
        let (result_tpe, result_span) = (self.tpd.tpe(result), self.tpd.span(result));
        args.push(self.type_tree(result_tpe, result_span, false));
        let binder = self.syms.fresh_binder();
        let method = self.u.intern_type(Type::Method {
            binder,
            params,
            result: result_tpe,
            implicit: true,
            contextual: true,
        });
        self.u.set_info(apply, method);
        let span = self.ast.span(first).union(result_span).to_synthetic();
        let arg_types: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let parent = self.u.intern_type(Type::Applied(class, arg_types));
        let tycon = self.type_tree(class, span.start_pos(), false);
        let args = self.tpd.list(&args);
        let core = self.tpd.node(T::AppliedTypeTree { tpt: tycon, args }, span, Some(parent), None);
        let def = self.build().def_def(apply, &[&param_trees], result, None, result_span);
        let tpe = self.u.intern_type(Type::Refined(parent, "apply".into(), method));
        let refinements = self.tpd.list(&[def]);
        Ok(self.tpd.node(T::RefinedTypeTree { tpt: core, refinements }, span, Some(tpe), None))
    }

    /// The type parameters, function and span of polymorphic function value `id` of type
    /// parameters `targs` and function `fun` with its context bounds eliminated: the evidence as
    /// a context function around the body, or joined to given parameters (dotty's
    /// `elimContextBounds` and `functionsOf`); `Err` for evidence a parameter refers to.
    pub fn poly_value_without_context_bounds(
        &mut self,
        id: NodeId,
        targs: List,
        fun: NodeId,
    ) -> Result<(List, NodeId, Span), &'static str> {
        let unchanged = (targs, fun, self.ast.span(id));
        let Tree::Function { args, body, mods: None, .. } = *self.ast.tree(fun) else { return Ok(unchanged) };
        let (targs, evidence) = self.poly_evidence(targs);
        if evidence.is_empty() {
            return Ok(unchanged);
        }
        let fun = match self.evidence_fit(&evidence, args) {
            Fit::Before => return Err("polymorphic function whose parameters refer to its evidence"),
            Fit::Joined => {
                let mut params = evidence;
                for p in self.ast.items(args).to_vec() {
                    let Tree::ValDef { name, tpt, .. } = *self.ast.tree(p) else { return Ok(unchanged) };
                    params.push((name, tpt));
                }
                self.contextual_closure(&params, body)
            }
            Fit::After => {
                let inner = self.contextual_closure(&evidence, body);
                let span = self.parsed_span(body);
                self.ast.with_span(inner, span);
                self.ast.node(Tree::Function { args, body: inner, mods: None, wildcard: false })
            }
        };
        let poly = self.ast.node(Tree::PolyFunction { targs, body: fun });
        Ok((targs, fun, self.ast.span(poly)))
    }

    /// The span of `t` as parsed: an infix operation's at its operator.
    fn parsed_span(&self, t: NodeId) -> Span {
        let span = self.ast.span(t);
        let Tree::Apply { fun, .. } = *self.ast.tree(t) else { return span };
        if !self.ast.attrs(t).contains(Attrs::WAS_TYPED_INFIX) {
            return span;
        }
        let op = self.ast.span(fun);
        Span::new(span.start(), span.end(), if op.is_synthetic() { op.start() } else { op.point() })
    }

    /// `(using params) ?=> body` (dotty's `makeContextualFunction`).
    fn contextual_closure(&mut self, params: &[Evidence], body: NodeId) -> NodeId {
        let params: Vec<NodeId> = params
            .iter()
            .map(|&(name, tpt)| {
                let mods = self.ast.mods(Modifiers::with_flags(f::GIVEN.or(f::PARAM)));
                self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods })
            })
            .collect();
        let args = self.ast.list(&params);
        self.ast.node(Tree::Function { args, body, mods: None, wildcard: false })
    }
}
