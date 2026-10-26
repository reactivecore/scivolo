// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicer.scala
//! The macro implementation a splice calls: a method of a static object, possibly inherited,
//! and whether its result is a context function of the `Quotes` the splice applies.

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
        let (mut clauses, mut context) = (Vec::new(), false);
        loop {
            match *self.tpd.tree(t) {
                T::Apply { fun, args } if clauses.is_empty() && !context => {
                    match self.context_function(fun, args, quotes) {
                        Some(f) => (context, t) = (true, f),
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
        let mut args = Vec::new();
        for arg in clauses.into_iter().rev().flatten() {
            let refused = (self.tpd.span(arg), "a macro argument other than a quote, a type or the Quotes");
            args.push(match self.macro_arg(arg, quotes).ok_or(refused)? {
                MacroArg::Expr(body) => {
                    if let Some(at) = self.new_instance_projection(body) {
                        return Err((at, "a macro argument selecting a parameter of a new instance"));
                    }
                    let body = self.constant_arg(body);
                    MacroArg::Expr(self.outer_inlined(body))
                }
                MacroArg::Type(t) => MacroArg::Type(crate::typer::simplify::normalize_all(self.u, t)),
                arg => arg,
            });
        }
        if context {
            args.push(MacroArg::Quotes);
        }
        let expansion = self.outermost_span;
        Ok(MacroCall { method, receiver, context, args, owner, span, expansion, searched: Vec::new() })
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
    /// if it is a context function of them, not taken by name.
    pub fn context_function(&mut self, fun: TNodeId, args: List, quotes: SymbolId) -> Option<TNodeId> {
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return None };
        let args = self.tpd.items(args);
        let apply = self.tpd.sym(fun)?;
        let class = self.u.symbol(apply).owner?;
        let is_quotes =
            |a: &TNodeId| self.tpd.sym(*a) == Some(quotes) && matches!(self.tpd.tree(*a), T::Ident { .. });
        let context =
            self.u.symbol(apply).name == "apply" && self.u.full_name(class) == "scala.ContextFunction1";
        if !context || !matches!(args, [a] if is_quotes(a)) {
            return None;
        }
        let f = self.u.widen(self.tpd.tpe(qualifier));
        let f = self.u.dealias(f);
        let types = self.u.types();
        let by_name = matches!(types.get(f), Type::Applied(_, targs)
            if targs.first().is_some_and(|&a| matches!(types.get(a), Type::ByName(_))));
        (!by_name).then_some(qualifier)
    }

    /// The span of a selection `new C(args).x` of a parameter `x` in `t`, which dotty's inliner
    /// reduces to the argument before the macro sees it (`InlineReducer.reduceProjection`).
    fn new_instance_projection(&mut self, t: TNodeId) -> Option<Span> {
        let mut todo = vec![t];
        while let Some(n) = todo.pop() {
            if let T::Select { qualifier, .. } = *self.tpd.tree(n)
                && let T::Apply { fun, .. } = *self.tpd.tree(qualifier)
                && let T::Select { qualifier: new, .. } = *self.tpd.tree(fun)
                && let T::New { tpt } = *self.tpd.tree(new)
                && let (Some(cls), Some(x)) = (self.u.class_symbol(self.tpd.tpe(tpt)), self.tpd.sym(n))
            {
                let s = self.u.symbol(x);
                let param =
                    s.kind == Kind::Param || s.kind == Kind::Val && s.flags.has(tasty::tags::PARAMsetter);
                if param && s.owner == Some(cls) {
                    return Some(self.tpd.span(n));
                }
            }
            todo.extend(self.tpd.children(n));
        }
        None
    }

    fn is_static_object(&self, c: SymbolId) -> bool {
        self.u.is_module_class(c) && self.u.is_static_symbol(c)
    }
}
