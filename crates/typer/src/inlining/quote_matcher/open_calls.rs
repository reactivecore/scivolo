// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! The calls of captured methods in a higher-order hole's closure, made calls of the functions
//! standing for them (the `Apply` case of `MatchResult.toExpr`'s tree map).

use super::super::Reflect;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashSet;
use syntax::ast::TName;
use tasty::{SymbolId, Type, TypeId};

impl Reflect<'_> {
    /// Rewrites the calls `f(args1)...(argsN)` in `t` of the parameters `functions`, which stand
    /// for methods, into `f.apply(args1)...apply(argsN)`.
    pub fn applies_through(&mut self, t: TNodeId, functions: &[SymbolId]) -> Result<(), &'static str> {
        let (mut todo, mut order) = (vec![t], Vec::new());
        while let Some(n) = todo.pop() {
            todo.extend(self.tpd.children(n));
            order.push(n);
        }
        let mut calls = FxHashSet::default();
        for n in order.into_iter().rev() {
            let (T::Apply { fun, args } | T::TypeApply { fun, args }) = *self.tpd.tree(n) else { continue };
            let ident = matches!(self.tpd.tree(fun), T::Ident { .. });
            let direct = self.tpd.sym(fun).filter(|f| ident && functions.contains(f));
            if direct.is_none() && !calls.contains(&fun) {
                continue;
            }
            calls.insert(n);
            if let Some(f) = direct {
                self.tpd.get_mut(fun).tpe = Some(self.u.intern_type(Type::Sym(None, f)));
            }
            let info = self.u.widen(self.tpd.tpe(fun));
            let fun = match self.u.method(info).is_some() || self.u.poly(info).is_some() {
                true => fun,
                false => self.select_apply(fun, info)?,
            };
            let args = self.tpd.items(args).to_vec();
            let call = match self.tpd.tree(n) {
                T::Apply { .. } => self.apply(fun, &args)?,
                _ => self.type_apply(fun, &args)?,
            };
            let call = self.tpd.get(call);
            let (tree, tpe) = (call.tree.clone(), call.tpe);
            let node = self.tpd.get_mut(n);
            (node.tree, node.tpe) = (tree, tpe);
        }
        Ok(())
    }

    /// `fun.apply` for `fun` of function type `info`.
    fn select_apply(&mut self, fun: TNodeId, info: TypeId) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(fun);
        if self.u.poly_function_apply_of(info).is_some() {
            let tpe = self.u.intern_type(Type::TermRef(info, "apply".into()));
            let name = TName { name: self.names.intern("apply"), is_type: false };
            return Ok(self.tpd.node(T::Select { qualifier: fun, name }, span, Some(tpe), None));
        }
        let apply =
            self.u.member_of_type(info, "apply").into_iter().find(|&m| self.u.symbol(m).kind.is_term());
        let apply = apply.ok_or("a captured method without a function type")?;
        Ok(self.build()?.select(fun, apply, span))
    }
}
