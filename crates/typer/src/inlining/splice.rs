//! Top-level splices of inlined macro bodies (dotty's `Splicer.splice` with its
//! `SpliceInterpreter`): the call of the macro implementation read off the splice, run by the
//! host, and the expression it returns built from its quotes.

use super::macros::{MacroArg, MacroCall, MacroError};
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, map_children};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::{Kind, SymbolId};

const RUNTIME_EXPR: &str = "scala.quoted.runtime.Expr$";

impl Inlining<'_> {
    /// The expansion of `t`, owned by `owner`, if it is a splice: the macro's result, retyped
    /// with its inline calls expanded.
    pub fn expand_splice(&mut self, t: TNodeId, owner: SymbolId) -> Result<Option<TNodeId>, Unsupported> {
        let Some(closure) = self.runtime_call(t, "splice") else { return Ok(None) };
        let span = self.tpd.span(t);
        let call = self.macro_call(closure, owner, span)?;
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        let expanded = match self.host.expand_macro(lent, &call) {
            Ok(expanded) => expanded,
            Err(MacroError::Unsupported(_)) => return Err((span, "a macro expansion the frontend refuses")),
            Err(MacroError::Failed(msg)) => {
                self.error(self.outermost_span, msg);
                return Ok(Some(t));
            }
        };
        let args: Vec<TNodeId> = call
            .args
            .iter()
            .filter_map(|a| match *a {
                MacroArg::Expr(t) => Some(t),
                _ => None,
            })
            .collect();
        let tree = self.quoted_trees(&expanded, &args)?;
        self.tpd.get_mut(tree).span = span;
        self.retype(tree, &FxHashMap::default(), owner).map(Some)
    }

    /// The argument of `t` if it is a call `scala.quoted.runtime.Expr.<name>[T](arg)`.
    fn runtime_call(&self, t: TNodeId, name: &str) -> Option<TNodeId> {
        let T::Apply { fun, args } = *self.tpd.tree(t) else { return None };
        let T::TypeApply { fun, .. } = *self.tpd.tree(fun) else { return None };
        let s = self.tpd.sym(fun)?;
        let owner = self.u.symbol(s).owner?;
        let found = self.u.symbol(s).name == name && self.u.full_name(owner) == RUNTIME_EXPR;
        found.then(|| self.tpd.items(args).first().copied()).flatten()
    }

    /// The call of the macro implementation that splice argument `closure`, a context function
    /// of the `Quotes`, makes.
    fn macro_call(
        &mut self,
        closure: TNodeId,
        owner: SymbolId,
        span: Span,
    ) -> Result<MacroCall, Unsupported> {
        let refused = (span, "a splice other than a call of a macro implementation");
        let T::Block { stats, .. } = *self.tpd.tree(closure) else { return Err(refused) };
        let [def] = self.tpd.items(stats)[..] else { return Err(refused) };
        let T::DefDef { paramss, rhs: Some(body), .. } = *self.tpd.tree(def) else { return Err(refused) };
        let params: Vec<TNodeId> =
            self.tpd.clause_items(paramss).iter().flat_map(|&l| self.tpd.items(l)).copied().collect();
        let [quotes] = params[..] else { return Err(refused) };
        let quotes = self.tpd.sym(quotes).ok_or(refused)?;
        let mut t = self.unwrapped(body);
        let mut clauses = Vec::new();
        loop {
            match *self.tpd.tree(t) {
                T::Apply { fun, args } => {
                    clauses.push(self.tpd.items(args).to_vec());
                    t = fun;
                }
                T::TypeApply { fun, .. } => t = fun,
                _ => break,
            }
        }
        let method = self.tpd.sym(t).filter(|&m| self.is_static_method(m)).ok_or(refused)?;
        let mut args = Vec::new();
        for arg in clauses.into_iter().rev().flatten() {
            let refused = (self.tpd.span(arg), "a macro argument other than a quote, a type or the Quotes");
            args.push(match self.macro_arg(arg, quotes).ok_or(refused)? {
                MacroArg::Expr(body) => MacroArg::Expr(self.outer_inlined(body)),
                arg => arg,
            });
        }
        Ok(MacroCall { method, args, owner, span })
    }

    /// Quoted expression `body` as an argument inlined from the outer scope, as the
    /// `SpliceInterpreter` wraps it.
    fn outer_inlined(&mut self, body: TNodeId) -> TNodeId {
        let body = self.without_outer_inlined(body);
        let (span, tpe) = (self.tpd.span(body), self.tpd.get(body).tpe);
        let bindings = self.tpd.list(&[]);
        self.tpd.node(T::Inlined { call: None, bindings, expansion: body }, span, tpe, None)
    }

    /// `t` with the arguments inlined from the outer scope in it unwrapped, as in dotty's quotes.
    fn without_outer_inlined(&mut self, t: TNodeId) -> TNodeId {
        if let T::Inlined { call: None, bindings, expansion } = *self.tpd.tree(t)
            && bindings.is_empty()
        {
            return self.without_outer_inlined(expansion);
        }
        map_children(self, t, &mut |i, c| i.without_outer_inlined(c));
        t
    }

    /// `t` without the typed, inlined and empty block trees around it.
    fn unwrapped(&self, t: TNodeId) -> TNodeId {
        match *self.tpd.tree(t) {
            T::Typed { expr, .. } => self.unwrapped(expr),
            T::Inlined { call: None, bindings, expansion } if bindings.is_empty() => {
                self.unwrapped(expansion)
            }
            T::Block { stats, expr } if stats.is_empty() => self.unwrapped(expr),
            _ => t,
        }
    }

    /// Whether `m` is a method of a static object, not inline.
    fn is_static_method(&self, m: SymbolId) -> bool {
        let s = self.u.symbol(m);
        let inline = s.flags.has(tasty::tags::INLINE);
        s.kind == Kind::Def
            && !inline
            && s.owner.is_some_and(|o| self.u.is_module_class(o) && self.u.is_static_symbol(o))
    }

    /// Argument `arg` of a macro implementation, given the splice's `Quotes` parameter `quotes`:
    /// `quotes`, `'{ e }(quotes)` or `Type.of[T](quotes)` (the `SpliceInterpreter`'s cases).
    fn macro_arg(&self, arg: TNodeId, quotes: SymbolId) -> Option<MacroArg> {
        let arg = self.unwrapped(arg);
        if self.tpd.sym(arg) == Some(quotes) && matches!(self.tpd.tree(arg), T::Ident { .. }) {
            return Some(MacroArg::Quotes);
        }
        let T::Apply { fun, args } = *self.tpd.tree(arg) else { return None };
        let [q] = self.tpd.items(args)[..] else { return None };
        if self.tpd.sym(self.unwrapped(q)) != Some(quotes) {
            return None;
        }
        match *self.tpd.tree(fun) {
            T::Select { qualifier, .. } => self.runtime_call(qualifier, "quote").map(MacroArg::Expr),
            T::TypeApply { fun, args } => {
                let of = self.tpd.sym(fun).is_some_and(|s| self.u.full_name(s) == "scala.quoted.Type$.of");
                let [tpt] = self.tpd.items(args)[..] else { return None };
                of.then(|| MacroArg::Type(self.tpd.tpe(tpt)))
            }
            _ => None,
        }
    }
}
