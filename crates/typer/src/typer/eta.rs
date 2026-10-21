// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//   compiler/src/dotty/tools/dotc/typer/Migrations.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Method references as function values (dotty's `etaExpand`): `m` as `(x1, ..., xn) => m(x1,
//! ..., xn)`, its impure prefix and arguments lifted into values before the function, its
//! parameter types from the method unless a function type of that arity is expected.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use syntax::diag::rewrite_notice;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The arity of the function method reference `tree` is eta-expanded to against `pt`, `None`
    /// if it is not (dotty's `arity` in `adaptNoArgs`).
    pub fn eta_arity(&mut self, tree: TNodeId, pt: Pt) -> Option<usize> {
        let tpe = self.tpd.tpe(tree);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Method { params, .. } = self.u.types().get(info).clone() else { return None };
        if let Pt::Type(p) = pt
            && let Some(n) = self.function_arity(p)
        {
            return Some(if !self.is_fully_defined(p) && self.is_fully_defined(info) { 0 } else { n });
        }
        let n = params.len();
        let varargs = params.last().is_some_and(|&(_, p)| self.repeated_elem(p).is_some());
        (n > 1 || n == 1 && !varargs || matches!(pt, Pt::AnyFunction)).then_some(n)
    }

    /// N if `t` is a `FunctionN` type.
    fn function_arity(&mut self, t: TypeId) -> Option<usize> {
        (0..=22).find(|&n| self.function_parts(t, n).is_some())
    }

    /// Whether `tree` refers to a constructor.
    pub fn is_constructor_ref(&mut self, tree: TNodeId) -> bool {
        self.tpd.sym(self.tpd.meth_part(tree)).is_some_and(|s| self.u.symbol(s).name == "<init>")
    }

    /// Whether `tree` refers to an inline implicit method, which is not eta-expanded.
    pub fn is_inline_implicit_ref(&mut self, tree: TNodeId) -> bool {
        self.tpd.sym(self.tpd.meth_part(tree)).is_some_and(|s| {
            let flags = &self.u.symbol(s).flags;
            flags.has(tasty::tags::INLINE) && flags.has(tasty::tags::IMPLICIT)
        })
    }

    /// Method reference `tree` eta-expanded against `pt` to a function of arity `xarity`.
    pub fn eta_expansion(
        &mut self,
        tree: TNodeId,
        pt: Pt,
        xarity: usize,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(tree);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Method { params, result, contextual: false, .. } = self.u.types().get(info).clone() else {
            return Err("method value");
        };
        if params.last().is_some_and(|&(_, p)| self.repeated_elem(p).is_some()) {
            return Err("method value with a repeated parameter");
        }
        if params.iter().any(|&(_, p)| matches!(self.u.types().get(p), Type::ByName(_))) {
            return Err("method value with a by-name parameter");
        }
        if self.tpd.sym(tree).is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::EXTENSION)) {
            return Err("extension method reference");
        }
        let last = match self.u.types().get(result) {
            Type::Method { implicit, .. } => *implicit,
            _ => true,
        };
        let function = match pt {
            Pt::Type(p) if self.function_parts(p, params.len()).is_some() => true,
            Pt::Type(p) if !matches!(self.sam_parts(p, params.len(), false), Ok(None)) => {
                return Err("method value of a single abstract method type");
            }
            _ => false,
        };
        let inferred = last && params.len() == xarity && function;
        let mut defs = Vec::new();
        let lifted = self.lift_app(&mut defs, tree, false, ctx)?;
        let lifted_tpe = self.tpd.tpe(lifted);
        let lifted_info = self.method_info(lifted_tpe).unwrap_or(lifted_tpe);
        let params = match self.u.types().get(lifted_info).clone() {
            Type::Method { params: lifted_params, .. } if lifted_params.len() == params.len() => {
                lifted_params
            }
            _ => params,
        };
        let java = self.tpd.sym(tree).is_some_and(|s| self.is_java_member(s));
        let span = self.tpd.span(tree);
        let whole = Span::synthetic(span.start(), span.end());
        let start = Span::synthetic(span.start(), span.start());
        let mut param_nodes = Vec::new();
        let mut ids = Vec::new();
        for (i, (name, p)) in params.iter().enumerate() {
            let tpt = self.ast.node(Tree::TypeTree);
            self.ast.with_span(tpt, start);
            if !inferred {
                let typed = self.type_tree(*p, start, false);
                self.splices.insert(tpt, typed);
            }
            let name = if java { self.name(&format!("x${i}")) } else { self.name(name) };
            let name = TName { name, is_type: false };
            let mods = self.ast.mods(Modifiers::with_flags(f::SYNTHETIC_TERM_PARAM));
            let param = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
            self.ast.with_span(param, start);
            param_nodes.push(param);
            let id = self.ast.node(Tree::Ident { name });
            self.ast.with_span(id, start);
            ids.push(id);
        }
        let fun = self.ast.node(Tree::TypeTree);
        self.ast.with_span(fun, self.tpd.span(lifted));
        self.splices.insert(fun, lifted);
        self.splice_owners.insert(fun, ctx.owner);
        let args = self.ast.list(&ids);
        let body = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(body, whole);
        let params = self.ast.list(&param_nodes);
        let function = self.ast.node(Tree::Function { args: params, body, mods: None, wildcard: false });
        self.ast.with_span(function, whole);
        let mut closure = self.typed_function(function, params, body, pt, ctx);
        if !defs.is_empty() {
            let locals: Vec<SymbolId> = defs.iter().filter_map(|&d| self.tpd.sym(d)).collect();
            closure = self
                .no_local_refs(closure, &locals, &[], pt)
                .ok_or("method value of a type referring to a lifted prefix")?;
            let block_span = defs.iter().fold(whole, |s, &d| s.union(self.tpd.span(d)));
            let stats = self.tpd.list(&defs);
            closure = self.build().block(stats, closure, block_span.to_synthetic());
        }
        Ok(self.adapt(closure, pt, ctx))
    }

    /// `qual _` (dotty's `typedAsFunction`): `qual` eta-expanded, or a function value already.
    pub fn typed_as_function(&mut self, id: NodeId, qual: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let pt1 = match pt {
            Pt::Type(p) if self.function_arity(p).is_some() => pt,
            _ => Pt::AnyFunction,
        };
        let res = self.typed(qual, pt1, ctx);
        let mut end = res;
        while let T::Block { expr, .. } = *self.tpd.tree(end) {
            end = expr;
        }
        if !matches!(self.tpd.tree(end), T::Closure { .. }) {
            let tpe = self.tpd.tpe(res);
            let widened = self.u.widen(tpe);
            if self.function_arity(widened).is_none() {
                return self.unsupported(id, "`_` after a value that is not a function");
            }
        }
        let span = self.ast.span(id);
        let msg = "The trailing ` _` for eta-expansion is unnecessary;\nthe compiler performs eta-expansion \
            automatically, so you can write the function value directly instead."
            .to_owned()
            + &rewrite_notice("This construct", "3.4-migration", "");
        self.diags.warning_at(span.start(), span.end(), span.point(), msg);
        res
    }

    /// Whether `tree` is a closure of a method whose body spans it, as eta-expansion makes, after
    /// lifted values or not.
    pub fn is_eta_closure(&mut self, tree: TNodeId) -> bool {
        let T::Block { stats, expr } = *self.tpd.tree(tree) else { return false };
        if matches!(self.tpd.tree(expr), T::Block { .. }) {
            return self.is_eta_closure(expr);
        }
        let (&[def], T::Closure { .. }) = (self.tpd.items(stats), self.tpd.tree(expr)) else { return false };
        let T::DefDef { rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return false };
        self.tpd.span(def).to_synthetic() == self.tpd.span(rhs)
    }

    /// Whether `s` is a member of a `java.*` or `javax.*` class or a Java class of the Scala
    /// library, its parameter names unknown.
    pub fn is_java_member(&mut self, s: SymbolId) -> bool {
        self.u.symbol(s).owner.is_some_and(|owner| self.is_java_class_name(owner))
    }

    /// Whether `c` is named as a `java.*` or `javax.*` class or a Java class of the Scala library.
    pub fn is_java_class_name(&mut self, c: SymbolId) -> bool {
        let name = self.u.full_name(c);
        let library_java = name
            .rsplit_once('.')
            .is_some_and(|(pkg, simple)| super::java_classes::java_classes(pkg).any(|c| c == simple));
        name.starts_with("java.") || name.starts_with("javax.") || library_java
    }
}
