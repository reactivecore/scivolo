// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! `Lambda.apply` and `Closure.apply` of `quotes.reflect`: the closure of a method a macro
//! defined.

use super::Reflect;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Binding, SymbolId, Type, TypeId};

impl Reflect<'_> {
    /// `{ def; closure(def) }` of method definition `def` with one clause of parameters (dotty's
    /// `tpd.Closure(meth, rhsFn)`), of the function type of the method's.
    pub fn closure(&mut self, def: TNodeId) -> Result<TNodeId, &'static str> {
        let meth = self.tpd.sym(def).filter(|_| matches!(self.tpd.tree(def), T::DefDef { .. }));
        let meth = meth.ok_or("Lambda.apply of another tree than a method definition")?;
        let closure = self.closure_of(meth)?;
        let (stats, span) = (self.tpd.list(&[def]), self.span);
        Ok(self.build()?.block(stats, closure, span))
    }

    /// The closure of method `meth` with one clause of parameters, of the function type of the
    /// method's (dotty's `tpd.Closure(Nil, meth, EmptyTree)`).
    pub fn closure_of(&mut self, meth: SymbolId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let info = self.u.info(meth);
        let Some(Binding { params, result, implicit: false, .. }) = self.u.method(info) else {
            return Err("Lambda.apply of another type than a method type");
        };
        if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
            return Err("Lambda.apply of a curried method type");
        }
        let mut types: Vec<TypeId> = params.iter().map(|&(_, t)| t).collect();
        let class = self.u.class_ref(&format!("scala.Function{}", types.len()));
        types.push(result);
        let fun_tpe = self.u.intern_type(Type::Applied(class, types));
        let meth_tpe = self.u.intern_type(Type::Sym(None, meth));
        let ident = self.build()?.ident(meth, meth_tpe, span);
        let env = self.tpd.list(&[]);
        Ok(self.tpd.node(T::Closure { env, meth: ident, tpt: None }, span, Some(fun_tpe), None))
    }
}
