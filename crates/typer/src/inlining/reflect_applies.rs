// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The applications `quotes.reflect` builds.

use super::Reflect;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::type_apply::instantiate_poly;
use tasty::tags::CLASSconst;
use tasty::{Binding, Type, TypeId};

impl Reflect<'_> {
    /// `Apply(fun, args)` (dotty's `tpd.Apply`), of the result of `fun`'s method type for the
    /// arguments' types.
    pub fn apply(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let info = self.u.widen(self.tpd.tpe(fun));
        let Some(Binding { binder, params, result, .. }) = self.u.method(info) else {
            return Err("Apply of a term that takes no arguments");
        };
        if params.len() != args.len() {
            return Err("Apply with another number of arguments than parameters");
        }
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let tpes: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let meth = self.tpd.sym(self.tpd.meth_part(fun));
        let tpe =
            crate::typer::dependent_params::applied_result(self.u, result, (binder, &names), meth, &tpes);
        Ok(self.build()?.apply(fun, args, tpe, span))
    }

    /// `Term.appliedToTypes(args)`.
    pub fn applied_to_types(&mut self, fun: TNodeId, args: &[TypeId]) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let refused = "appliedToTypes of a term that takes no such type arguments";
        if let (Some(f), &[arg]) = (self.tpd.sym(fun), args)
            && self.u.full_name(f) == "scala.Predef$.classOf"
        {
            let tpe = self.u.intern_type(Type::Constant(CLASSconst, i64::from(arg.0)));
            return Ok(self.tpd.node(T::ClassLiteral { tpe: arg }, span, Some(tpe), None));
        }
        let tpts: Vec<TNodeId> = args
            .iter()
            .map(|&a| self.tpd.node(T::TypeTree { inferred: false }, span, Some(a), None))
            .collect();
        self.type_apply(fun, &tpts).map_err(|_| refused)
    }

    /// `TypeApply(fun, tpts)`, of `fun`'s polymorphic type instantiated with the trees' types.
    pub fn type_apply(&mut self, fun: TNodeId, tpts: &[TNodeId]) -> Result<TNodeId, &'static str> {
        let refused = "TypeApply of a term that takes no such type arguments";
        let info = self.u.widen(self.tpd.tpe(fun));
        let Some(Binding { binder, params, result, .. }) = self.u.poly(info) else {
            return Err(refused);
        };
        if params.len() != tpts.len() {
            return Err(refused);
        }
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let args: Vec<TypeId> = tpts.iter().map(|&t| self.tpd.tpe(t)).collect();
        let tpe = instantiate_poly(self.u, result, (binder, &names), &args, self.tpd.sym(fun));
        let span = self.span;
        Ok(self.build()?.type_apply(fun, tpts, tpe, span))
    }
}
