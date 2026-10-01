//! Applications `f(args)` and `new C` (dotty's `typedApply`, `typedNew`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type of the method `fun_tpe` refers to, as seen from its prefix.
    pub fn method_info(&mut self, fun_tpe: TypeId) -> Option<TypeId> {
        let Type::Sym(prefix, s) = self.u.types().get(fun_tpe).clone() else { return None };
        self.complete(s);
        let seen_from_prefix = prefix.filter(|&p| !matches!(self.u.types().get(p), Type::Package(_)));
        Some(match seen_from_prefix {
            Some(p) => self.u.member_info(p, s),
            None => self.u.info(s),
        })
    }

    pub fn typed_apply(&mut self, id: NodeId, fun: NodeId, args: List, ctx: Ctx) -> TNodeId {
        let arg_ids = self.ast.items(args).to_vec();
        let proto = self.protos.len();
        self.protos.push(super::overloads::FunProto { args: arg_ids.clone(), ctx, typed: None });
        let f = self.typed(fun, Pt::Fun(proto), ctx);
        let pretyped = self.protos[proto].typed.take();
        let fun_tpe = self.tpd.tpe(f);
        let info = self.method_info(fun_tpe);
        let Some(Type::Method { params, result, .. }) = info.map(|i| self.u.types().get(i).clone()) else {
            return self.unsupported(id, "application of a non-method");
        };
        if params.len() != arg_ids.len() {
            return self.unsupported(id, "default or variable arguments");
        }
        if params.iter().any(|&(_, p)| self.is_special_param(p)) {
            return self.unsupported(id, "by-name, repeated or dependent parameter");
        }
        match self.u.types().get(result) {
            Type::Bounds(..) => return self.unsupported(id, "result type of a wildcard argument"),
            Type::Method { implicit: true, .. } => return self.unsupported(id, "implicit arguments"),
            _ => {}
        }
        let typed_args: Vec<TNodeId> = match pretyped {
            Some(typed) => {
                typed.into_iter().zip(&params).map(|(a, &(_, p))| self.adapt(a, Pt::Type(p))).collect()
            }
            None => {
                arg_ids.iter().zip(&params).map(|(&a, &(_, p))| self.typed(a, Pt::Type(p), ctx)).collect()
            }
        };
        let tpe = match self.u.types().get(fun_tpe).clone() {
            Type::Sym(Some(class), s)
                if self.u.symbol(s).name == "<init>" && self.u.symbol(s).kind == Kind::Def =>
            {
                if matches!(self.u.types().get(class), Type::This(_)) {
                    return self.unsupported(id, "secondary constructor call");
                }
                self.constructor_result(s, class, result)
            }
            _ => result,
        };
        let args = self.tpd.list(&typed_args);
        let span = self.ast.span(id);
        let apply = self.tpd.node(T::Apply { fun: f, args }, span, Some(tpe), None);
        self.fold_apply(apply);
        apply
    }

    /// The type of a call of constructor `ctor` on `new` of type `class`, whose method type gives
    /// `result`: `result` if it names the constructor's class, else the class itself unless
    /// `class` applies it to arguments.
    pub fn constructor_result(&mut self, ctor: SymbolId, class: TypeId, result: TypeId) -> TypeId {
        let owner = self.u.symbol(ctor).owner;
        let names_owner = |u: &tasty::Universe, t| match *u.types().get(t) {
            Type::Sym(_, c) => Some(c) == owner,
            Type::Applied(c, _) => matches!(*u.types().get(c), Type::Sym(_, c) if Some(c) == owner),
            _ => false,
        };
        match owner {
            _ if names_owner(self.u, result) => result,
            Some(o) if !matches!(self.u.types().get(class), Type::Applied(..)) => {
                crate::defn::type_ref(self.u, o)
            }
            _ => class,
        }
    }

    /// Whether parameter type `p` is by-name, repeated, or refers to a parameter.
    fn is_special_param(&mut self, p: TypeId) -> bool {
        let mut special = false;
        self.u.map_type(p, &mut |u, part| {
            match u.types().get(part) {
                Type::ByName(_) | Type::Param(..) => special = true,
                Type::Sym(_, s) if u.symbol(*s).name == "<repeated>" => special = true,
                Type::TypeRef(_, n) if n == "<repeated>" => special = true,
                _ => {}
            }
            None
        });
        special
    }

    pub fn typed_new(&mut self, id: NodeId, tpt: NodeId, ctx: Ctx) -> TNodeId {
        let tpt = self.typed_type(tpt, ctx);
        let tpe = self.tpd.tpe(tpt);
        let span = self.ast.span(id);
        self.tpd.node(T::New { tpt }, span, Some(tpe), None)
    }
}
