//! Explicit type applications `f[T]` (dotty's `typedTypeApply`), their arguments checked
//! against the bounds of the method's type parameters.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Type, TypeId};

impl Typer<'_> {
    pub fn typed_type_apply(&mut self, id: NodeId, fun: NodeId, args: List, ctx: Ctx) -> TNodeId {
        let f = self.typed(fun, Pt::PolyFun, ctx);
        if self.tpd.sym(f).is_some_and(|s| self.u.full_name(s) == "scala.Predef$.classOf") {
            return self.unsupported(id, "classOf");
        }
        let fun_tpe = self.tpd.tpe(f);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return self.unsupported(id, "type application of a non-polymorphic method");
        };
        let arg_ids = self.ast.items(args).to_vec();
        if arg_ids.len() != params.len() {
            return self.unsupported(id, "wrong number of type arguments");
        }
        let typed_args: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
        let arg_types: Vec<TypeId> = typed_args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let method = self.tpd.sym(f);
        let instantiate = |t: &mut Self, x: TypeId| {
            let x = t.u.subst(x, binder, &names, &arg_types);
            t.u.map_type(x, &mut |u, part| match *u.types().get(part) {
                Type::Sym(_, s) if method.is_some() && u.symbol(s).owner == method => {
                    let name = &u.symbol(s).name;
                    names.iter().position(|n| n == name).map(|i| arg_types[i])
                }
                _ => None,
            })
        };
        for (&(_, bounds), &arg) in params.iter().zip(&arg_types) {
            let bounds = instantiate(self, bounds);
            let (lo, hi) = match *self.u.types().get(bounds) {
                Type::Bounds(lo, hi) => (lo, hi),
                _ => (bounds, bounds),
            };
            if !self.u.is_subtype(lo, arg) || !self.u.is_subtype(arg, hi) {
                return self.unsupported(id, "type argument outside its bounds");
            }
        }
        let tpe = instantiate(self, result);
        let args = self.tpd.list(&typed_args);
        let span = self.ast.span(id);
        self.tpd.node(T::TypeApply { fun: f, args }, span, Some(tpe), None)
    }
}
