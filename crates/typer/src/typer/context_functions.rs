// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Context functions: a term expected to be one wrapped in a context closure (dotty's
//! `makeContextualFunction`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Type, TypeId, TypeVars};

/// How many nested context function results or variable bounds are followed.
const MAX_NESTING: u32 = 32;

impl Typer<'_> {
    /// Whether parameter `p` is a `given` one.
    pub fn is_given_param(&self, p: NodeId) -> bool {
        matches!(*self.ast.tree(p), Tree::ValDef { mods, .. } if self.ast.modifiers(mods).flags.is(f::GIVEN))
    }

    /// `id` typed against context function type `pt` as `(contextual$1: A, …) ?=> id` (dotty's
    /// `makeContextualFunction`, the check of `typedUnadapted` with `isApplyProto`); `None` where
    /// dotty types `id` as it is.
    pub fn contextual_function(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> Option<TNodeId> {
        let Pt::Type(p) = pt else { return None };
        if ctx.mode.is(Mode::PATTERN)
            || self.ahead.get(&id).is_some_and(|e| e.iter().any(|&(o, _)| o == ctx.owner))
            || !self.is_term_tree(id)
            || self.is_contextual_closure(id)
        {
            return None;
        }
        let (ifpt, formals) = match self.as_context_function(p) {
            Ok(found) => found?,
            Err(what) => return Some(self.unsupported(id, what)),
        };
        if self.u.symbol(ctx.owner).name.contains("$default$") {
            return Some(self.unsupported(id, "default argument of a context function type"));
        }
        let wildcard = formals.iter().any(|&t| self.has_proto_wildcard(t));
        let span = self.ast.span(id);
        let at = span.start_pos();
        let applied = self.dependent_context_function(ifpt).or_else(|| self.erased_context_names(ifpt));
        let mut names = applied.into_iter().flat_map(|(ps, _)| ps).map(|(n, _)| n);
        let erased = self.erased_context_function(ifpt).map(|(_, _, e)| e).unwrap_or_default();
        let mut params = Vec::new();
        for (i, formal) in formals.into_iter().enumerate() {
            let tpt = self.ast.node(Tree::TypeTree);
            self.ast.with_span(tpt, at);
            if !wildcard {
                let formal = match *self.u.types().get(formal) {
                    Type::Bounds(lo, _) => lo,
                    _ => formal,
                };
                let typed = self.type_tree(formal, at, true);
                self.splices.insert(tpt, typed);
            }
            let named = names.next();
            let name = match &named {
                Some(n) => self.name(n),
                None => self.fresh.next(self.names, "contextual$"),
            };
            let name = TName { name, is_type: false };
            let flags = if erased.get(i) == Some(&true) { f::GIVEN.or(f::ERASED) } else { f::GIVEN };
            let mods = self.ast.mods(Modifiers::with_flags(flags));
            let param = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
            if named.is_some() {
                self.hidden_params.insert(param);
            }
            self.ast.with_span(param, at);
            params.push(param);
        }
        let args = self.ast.list(&params);
        let function = self.ast.node(Tree::Function { args, body: id, mods: None, wildcard: false });
        self.ast.with_span(function, span.to_synthetic());
        Some(self.typed_function(function, args, id, Pt::Type(ifpt), ctx))
    }

    /// Refuses default argument `rhs` typed without its parameter's type `unexpected`, a context
    /// function type, which dotty expects with wildcards.
    pub fn check_context_default(&mut self, rhs: NodeId, unexpected: Option<TypeId>) {
        if unexpected.is_some_and(|t| self.is_context_function_type(t)) {
            self.unsupported(rhs, "default argument of a context function type");
        }
    }

    /// Whether `t` is a context function type (dotty's `isContextFunctionType`).
    pub fn is_context_function_type(&mut self, t: TypeId) -> bool {
        !matches!(self.as_context_function(t), Ok(None))
    }

    /// `t` with context function types replaced by their results, repeatedly (dotty's
    /// `normalize` of a value type).
    pub fn context_result(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..MAX_NESTING {
            match self.context_function_parts(t) {
                Some((_, result)) => t = result,
                None => break,
            }
        }
        t
    }

    /// The context function type `t` is, a type variable's upper bound, dealiased, with its
    /// parameter types (dotty's `asContextFunctionType`); `Err` for one the subset leaves out.
    fn as_context_function(&mut self, t: TypeId) -> Result<Option<(TypeId, Vec<TypeId>)>, &'static str> {
        let mut t = self.zonk(t);
        for _ in 0..MAX_NESTING {
            let Type::Var(v) = *self.u.types().get(t) else { break };
            t = self.constraint.bounds(v).1;
            t = self.zonk(t);
        }
        let d = self.u.dealias(t);
        if let Some((params, _)) = self.context_function_parts(d) {
            return Ok(Some((d, params)));
        }
        if let Some((params, _, _)) = self.erased_context_function(d) {
            return Ok(Some((d, params)));
        }
        match self.dependent_context_function(d) {
            Some((params, result)) if self.context_function_parts(result).is_none() => {
                Ok(Some((d, params.into_iter().map(|(_, p)| p).collect())))
            }
            Some(_) => Err("curried dependent context function type"),
            None => Ok(None),
        }
    }

    /// Whether `id` is a term, or stands for a typed one.
    fn is_term_tree(&self, id: NodeId) -> bool {
        match self.ast.tree(id) {
            Tree::TypedSplice { sym } => !sym.is_type,
            Tree::TypeTree => self.splices.get(&id).is_some_and(|&t| {
                !matches!(
                    self.tpd.tree(t),
                    T::TypeTree { .. }
                        | T::AppliedTypeTree { .. }
                        | T::SingletonTypeTree { .. }
                        | T::ByNameTypeTree { .. }
                        | T::TypeBoundsTree { .. }
                        | T::LambdaTypeTree { .. }
                        | T::RefinedTypeTree { .. }
                )
            }),
            _ => self.ast.is_term(id),
        }
    }

    /// Whether `id` is a context function value, possibly in a block (dotty's
    /// `isContextualClosure`).
    fn is_contextual_closure(&self, id: NodeId) -> bool {
        match *self.ast.tree(id) {
            Tree::Function { args, .. } => {
                self.ast.items(args).first().is_some_and(|&p| self.is_given_param(p))
            }
            Tree::Block { stats, expr: Some(e) } if self.ast.items(stats).is_empty() => {
                self.is_contextual_closure(e)
            }
            _ => false,
        }
    }
}
