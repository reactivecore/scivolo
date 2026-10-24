// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicer.scala
//! Top-level splices of inlined macro bodies (dotty's `Splicer.splice` with its
//! `SpliceInterpreter`): the call of the macro implementation read off the splice, run by the
//! host, and the expression it returns built from its quotes.

use super::macros::{MacroArg, MacroError};
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, map_children};
use rustc_hash::FxHashMap;
use tasty::tags::INLINEPROXY;
use tasty::{Kind, SymbolId};

const RUNTIME_EXPR: &str = "scala.quoted.runtime.Expr$";
/// The implicit searches of one expansion, each a run of the macro.
const MAX_SEARCHES: usize = 64;
const ABORTED: &str = "Macro expansion was aborted by the macro without any errors reported. Macros should issue \
                       errors to end-users when aborting a macro expansion with StopMacroExpansion.";

impl Inlining<'_> {
    /// The expansion of `t`, owned by `owner`, if it is a splice: the macro's result, retyped
    /// with its inline calls expanded.
    pub fn expand_splice(&mut self, t: TNodeId, owner: SymbolId) -> Result<Option<TNodeId>, Unsupported> {
        let Some(closure) = self.runtime_call(t, "splice") else { return Ok(None) };
        let span = self.tpd.span(t);
        let mut call = self.macro_call(closure, owner, span)?;
        let expanded = loop {
            let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
            match self.host.expand_macro(lent, &call) {
                Err(MacroError::Search(pt)) if call.searched.len() < MAX_SEARCHES => {
                    let found = self.summon(pt, self.outermost_span)?;
                    call.searched.push((pt, found));
                }
                expanded => break expanded,
            }
        };
        let expanded = match expanded {
            Ok(expanded) => expanded,
            Err(MacroError::Search(_)) => {
                return Err((span, "a macro searching implicit arguments repeatedly"));
            }
            Err(MacroError::Unsupported(_)) => return Err((span, "a macro expansion the frontend refuses")),
            Err(MacroError::Failed(msg)) => {
                self.error(self.outermost_span, msg);
                return Ok(Some(t));
            }
            Err(MacroError::Reported(errors)) => {
                for (at, msg) in errors {
                    self.error(at, msg);
                }
                return Ok(Some(t));
            }
            Err(MacroError::Aborted) => {
                if !self.host.has_errors() {
                    self.error(span, ABORTED.to_owned());
                }
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
        let [tree] = self.quoted_trees(&expanded, &args)?[..] else {
            return Err((span, "a splice returning other than one expression"));
        };
        self.check_macro_tree(tree, owner)?;
        self.tpd.get_mut(tree).span = span;
        let outer = std::mem::replace(&mut self.in_macro_result, true);
        let retyped = self.retype(tree, &FxHashMap::default(), owner);
        self.in_macro_result = outer;
        retyped.map(Some)
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

    /// Quoted argument `body` as a literal if it refers to an inline proxy whose type normalizes
    /// to a constant (dotty's `constToLiteral` of an inline parameter's binding).
    pub fn constant_arg(&mut self, body: TNodeId) -> TNodeId {
        let inner = self.unwrapped(body);
        let proxy =
            |u: &tasty::Universe, s| u.symbol(s).kind == Kind::Val && u.symbol(s).flags.has(INLINEPROXY);
        let Some(s) = self.tpd.sym(inner).filter(|&s| proxy(self.u, s)) else { return body };
        if !matches!(self.tpd.tree(inner), T::Ident { .. }) {
            return body;
        }
        let info = self.u.info(s);
        let info = self.u.try_normalize(info).unwrap_or(info);
        match self.constant_of_type(info) {
            Some(value) => self.literal(value, self.tpd.span(inner)),
            None => body,
        }
    }

    /// Quoted expression `body` as an argument inlined from the outer scope, as the
    /// `SpliceInterpreter` wraps it.
    pub fn outer_inlined(&mut self, body: TNodeId) -> TNodeId {
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
    pub fn unwrapped(&self, t: TNodeId) -> TNodeId {
        match *self.tpd.tree(t) {
            T::Typed { expr, .. } => self.unwrapped(expr),
            T::Inlined { call: None, bindings, expansion } if bindings.is_empty() => {
                self.unwrapped(expansion)
            }
            T::Block { stats, expr } if stats.is_empty() => self.unwrapped(expr),
            _ => t,
        }
    }

    /// Argument `arg` of a macro implementation, given the splice's `Quotes` parameter `quotes`:
    /// `quotes`, `'{ e }(quotes)` or `Type.of[T](quotes)` (the `SpliceInterpreter`'s cases).
    pub fn macro_arg(&self, arg: TNodeId, quotes: SymbolId) -> Option<MacroArg> {
        let arg = self.unwrapped(arg);
        if self.tpd.sym(arg) == Some(quotes) && matches!(self.tpd.tree(arg), T::Ident { .. }) {
            return Some(MacroArg::Quotes);
        }
        if let T::Literal { value } = self.tpd.tree(arg) {
            return Some(MacroArg::Literal(value.clone()));
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
