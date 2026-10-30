// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicer.scala
//! The macro implementation a splice calls: a method of a static object, possibly inherited,
//! and how many context functions of the `Quotes` the splice applies to its result.

use super::macros::{MacroArg, MacroCall};
use super::{Inlining, Unsupported};
use crate::tpd::{List, TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId, Type};

impl Inlining<'_> {
    /// The call of the macro implementation that splice argument `closure`, a context function
    /// of the `Quotes`, makes.
    pub fn macro_call(
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
        let mut bound = Vec::new();
        if let T::Block { stats, expr } = *self.tpd.tree(t) {
            for &stat in self.tpd.items(stats) {
                let (T::ValDef { rhs: Some(rhs), .. }, Some(s)) = (self.tpd.tree(stat), self.tpd.sym(stat))
                else {
                    return Err(refused);
                };
                if !self.u.symbol(s).flags.has(tasty::tags::SYNTHETIC) {
                    return Err(refused);
                }
                bound.push((s, *rhs));
            }
            t = self.unwrapped(expr);
        }
        let (mut clauses, mut context) = (Vec::new(), 0);
        loop {
            match *self.tpd.tree(t) {
                T::Apply { fun, args } if clauses.is_empty() => {
                    match self.context_function(fun, args, quotes) {
                        Some(f) => (context, t) = (context + 1, f),
                        None => {
                            clauses.push(self.tpd.items(args).to_vec());
                            t = fun;
                        }
                    }
                }
                T::Apply { fun, args } => {
                    clauses.push(self.tpd.items(args).to_vec());
                    t = fun;
                }
                T::TypeApply { fun, .. } => t = fun,
                _ => break,
            }
        }
        let (method, receiver) = self.macro_method(t).ok_or(refused)?;
        let erased = self.erased_params(method);
        let mut args = Vec::new();
        let clauses = clauses.into_iter().rev().flatten();
        for (arg, _) in clauses.zip(erased.into_iter().chain(std::iter::repeat(false))).filter(|a| !a.1) {
            let refused = (self.tpd.span(arg), "a macro argument other than a quote, a type or the Quotes");
            let arg = match *self.tpd.tree(arg) {
                T::NamedArg { arg, .. } => arg,
                _ => arg,
            };
            let arg = match (self.tpd.tree(arg), self.tpd.sym(arg)) {
                (T::Ident { .. }, Some(s)) => {
                    bound.iter().find(|&&(b, _)| b == s).map_or(arg, |&(_, rhs)| rhs)
                }
                _ => arg,
            };
            let arg = self.macro_arg(arg, quotes).ok_or(refused)?;
            args.push(self.prepared_arg(arg)?);
        }
        args.extend((0..context).map(|_| MacroArg::Quotes));
        let expansion = self.outermost_span;
        let while_typing = self.transparent_only;
        Ok(MacroCall {
            method,
            receiver,
            context,
            args,
            owner,
            span,
            expansion,
            searched: Vec::new(),
            while_typing,
        })
    }

    /// The macro implementation reference `t` names and the static object it is called on: the
    /// method's owner, or an object inheriting it (dotty's `interpretedStaticMethodCall`).
    pub fn macro_method(&mut self, t: TNodeId) -> Option<(SymbolId, SymbolId)> {
        let m = self.tpd.sym(t)?;
        let s = self.u.symbol(m);
        if s.kind != Kind::Def || s.flags.has(INLINE) {
            return None;
        }
        let owner = s.owner?;
        if self.is_static_object(owner) {
            return Some((m, owner));
        }
        let prefix = match *self.u.types().get(self.tpd.tpe(t)) {
            Type::Sym(Some(p), _) | Type::TermRef(p, _) => p,
            _ => return None,
        };
        let object = match *self.u.types().get(prefix) {
            Type::Sym(_, v) if self.u.is_module(v) => self.u.module_class(v)?,
            Type::This(c) => self.u.class_symbol(c)?,
            _ => return None,
        };
        (self.is_static_object(object) && self.u.derives_from(object, owner)).then_some((m, object))
    }

    /// The function `fun` applied to the splice's `Quotes` `quotes` in `args` calls `apply` on,
    /// if it is a context function of them.
    pub fn context_function(&mut self, fun: TNodeId, args: List, quotes: SymbolId) -> Option<TNodeId> {
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return None };
        let args = self.tpd.items(args);
        let apply = self.tpd.sym(fun)?;
        let class = self.u.symbol(apply).owner?;
        let is_quotes =
            |a: &TNodeId| self.tpd.sym(*a) == Some(quotes) && matches!(self.tpd.tree(*a), T::Ident { .. });
        let context =
            self.u.symbol(apply).name == "apply" && self.u.full_name(class) == "scala.ContextFunction1";
        (context && matches!(args, [a] if is_quotes(a))).then_some(qualifier)
    }

    /// Whether each parameter of method `m`, through all its clauses, is erased (dotty's
    /// `paramErasureStatuses`).
    fn erased_params(&mut self, m: SymbolId) -> Vec<bool> {
        let (mut t, mut erased) = (self.u.info(m), Vec::new());
        loop {
            if let Type::Poly { result, .. } = *self.u.types().get(t) {
                t = result;
            }
            let Some(binding) = self.u.method(t) else { return erased };
            let (params, result) = (binding.param_types(), binding.result);
            erased.extend(params.into_iter().map(|p| self.u.is_erased_param(p)));
            t = result;
        }
    }

    fn is_static_object(&self, c: SymbolId) -> bool {
        self.u.is_module_class(c) && self.u.is_static_symbol(c)
    }
}
