//! Method references as function values (dotty's `etaExpand`): `m` as `(x1, ..., xn) => m(x1,
//! ..., xn)`, its parameter types from the method unless a function type of that arity is
//! expected.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{Span, TName, Tree};
use tasty::{SymbolId, Type};

impl Typer<'_> {
    /// Method reference `tree` eta-expanded against `pt`.
    pub fn eta_expansion(&mut self, tree: TNodeId, pt: Pt, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let pure = match *self.tpd.tree(tree) {
            T::Ident { .. } => true,
            T::Select { qualifier, .. } => self.is_pure_expr(qualifier),
            _ => false,
        };
        if !pure {
            return Err("method value of an impure prefix or partial application");
        }
        let tpe = self.tpd.tpe(tree);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Method { params, result, contextual: false, .. } = self.u.types().get(info).clone() else {
            return Err("method value");
        };
        if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
            return Err("method value of several parameter lists");
        }
        if params.last().is_some_and(|&(_, p)| self.repeated_elem(p).is_some()) {
            return Err("method value with a repeated parameter");
        }
        if params.iter().any(|&(_, p)| matches!(self.u.types().get(p), Type::ByName(_))) {
            return Err("method value with a by-name parameter");
        }
        if self.tpd.sym(tree).is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::EXTENSION)) {
            return Err("extension method reference");
        }
        let inferred = match pt {
            Pt::Type(p) if self.function_parts(p, params.len()).is_some() => {
                self.is_fully_defined(p) || !self.is_fully_defined(info)
            }
            Pt::Type(p) if !matches!(self.sam_parts(p, params.len()), Ok(None)) => {
                return Err("method value of a single abstract method type");
            }
            _ => false,
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
        self.ast.with_span(fun, span);
        self.splices.insert(fun, tree);
        let args = self.ast.list(&ids);
        let body = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(body, whole);
        let params = self.ast.list(&param_nodes);
        let function = self.ast.node(Tree::Function { args: params, body, mods: None, wildcard: false });
        self.ast.with_span(function, whole);
        let closure = self.typed_function(function, params, body, pt, ctx);
        Ok(self.adapt(closure, pt, ctx))
    }

    /// Whether `tree` is a closure of a method whose body spans it, as eta-expansion makes.
    pub fn is_eta_closure(&mut self, tree: TNodeId) -> bool {
        let T::Block { stats, expr } = *self.tpd.tree(tree) else { return false };
        let (&[def], T::Closure { .. }) = (self.tpd.items(stats), self.tpd.tree(expr)) else { return false };
        let T::DefDef { rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return false };
        self.tpd.span(def).to_synthetic() == self.tpd.span(rhs)
    }

    /// Whether `s` is a member of a `java.*` or `javax.*` class or a Java class of the Scala
    /// library, its parameter names unknown.
    pub fn is_java_member(&mut self, s: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(s).owner else { return false };
        let name = self.u.full_name(owner);
        let library_java = name
            .rsplit_once('.')
            .is_some_and(|(pkg, simple)| super::java_classes::java_classes(pkg).any(|c| c == simple));
        name.starts_with("java.") || name.starts_with("javax.") || library_java
    }
}
