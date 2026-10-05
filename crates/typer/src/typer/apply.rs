//! Applications `f(args)` and `new C` (dotty's `typedApply`, `typedNew`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_apply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let arg_ids = self.ast.items(args).to_vec();
        let proto = self.protos.len();
        self.protos.push(super::overloads::FunProto { args: arg_ids.clone(), ctx, typed: None });
        let f = self.typed(fun, Pt::Fun(proto), ctx);
        let f = match self.with_apply(f, proto, ctx) {
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        let method = self.tpd.sym(f);
        let f = match self.with_type_vars(f) {
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        let pretyped = self.protos[proto].typed.take();
        let fun_tpe = self.tpd.tpe(f);
        let info = self.method_info(fun_tpe).or(Some(fun_tpe));
        let Some(Type::Method { binder, mut params, result, .. }) =
            info.map(|i| self.u.types().get(i).clone())
        else {
            return self.unsupported(id, "application of a non-method");
        };
        if self.mentions_params(result, binder, method) {
            return self.unsupported(id, "dependent result type");
        }
        if let Pt::Type(p) = pt
            && !matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. })
        {
            self.conforms(result, p);
        }
        let repeated = params.last().and_then(|&(_, p)| self.repeated_elem(p));
        if repeated.is_some() {
            params.pop();
        }
        let arity_fits = match repeated {
            Some(_) => arg_ids.len() >= params.len(),
            None => arg_ids.len() == params.len(),
        };
        if !arity_fits {
            return self.unsupported(id, "default arguments or wrong arity");
        }
        let formals: Vec<TypeId> = params
            .iter()
            .map(|&(_, p)| p)
            .chain(repeated)
            .map(|p| match *self.u.types().get(p) {
                Type::ByName(r) => r,
                _ => p,
            })
            .collect();
        if formals.iter().any(|&p| self.is_special_param(p)) {
            return self.unsupported(id, "dependent parameter");
        }
        if arg_ids.iter().any(|&a| self.is_sequence_arg(a)) {
            return self.unsupported(id, "sequence argument");
        }
        if let Type::Bounds(..) = self.u.types().get(result) {
            return self.unsupported(id, "result type of a wildcard argument");
        }
        let mut typed_args = Vec::new();
        for (i, &a) in arg_ids.iter().enumerate() {
            let formal = formals[i.min(formals.len() - 1)];
            typed_args.push(match &pretyped {
                Some(typed) => self.adapt(typed[i], Pt::Type(formal), ctx),
                None => self.typed(a, Pt::Type(formal), ctx),
            });
        }
        let span = self.ast.span(id);
        if let Some(elem) = repeated {
            let elems = typed_args.split_off(params.len());
            let before = typed_args.last().copied().unwrap_or(f);
            let at = self.tpd.span(before).end_pos();
            typed_args.push(self.seq_literal(&elems, elem, at));
        }
        if let [arg] = typed_args[..]
            && let Err(what) = self.check_can_equal(f, self.tpd.tpe(arg))
        {
            return self.unsupported(id, what);
        }
        let mut callee = f;
        while let T::Apply { fun, .. } | T::TypeApply { fun, .. } = *self.tpd.tree(callee) {
            callee = fun;
        }
        let callee_tpe = self.tpd.tpe(callee);
        let partial = matches!(self.u.types().get(result), Type::Method { .. });
        let tpe = match self.u.types().get(callee_tpe).clone() {
            Type::Sym(Some(class), s)
                if self.u.symbol(s).name == "<init>" && self.u.symbol(s).kind == Kind::Def =>
            {
                if matches!(self.u.types().get(class), Type::This(_)) {
                    return self.unsupported(id, "secondary constructor call");
                }
                if partial { result } else { self.constructor_result(s, class, result) }
            }
            _ => result,
        };
        let args = self.tpd.list(&typed_args);
        let apply = self.tpd.node(T::Apply { fun: f, args }, span, Some(tpe), None);
        self.fold_apply(apply);
        apply
    }

    /// `f`, or `f.apply` if `f` is not a method (dotty's `tryInsertApplyOrImplicit`), the
    /// alternative chosen by the arguments of `proto`.
    fn with_apply(&mut self, f: TNodeId, proto: usize, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. } | Type::Error) {
            return Ok(f);
        }
        let tpe = self.widen_if_unstable(tpe);
        let found = self.members_found(tpe, "apply", false).ok_or("application of a non-method")?;
        let sym = self.choose(&found.syms, Some(tpe), super::Pt::Fun(proto))?;
        self.check_reference(sym, Some(tpe), ctx)?;
        let name = syntax::ast::TName { name: self.name("apply"), is_type: false };
        let apply_tpe = self.ref_type(Some(tpe), sym);
        let span = self.tpd.span(f).to_synthetic();
        Ok(self.tpd.node(T::Select { qualifier: f, name }, span, Some(apply_tpe), Some(sym)))
    }

    /// Whether argument `a` is a sequence passed to a repeated parameter, `xs*`.
    fn is_sequence_arg(&self, a: NodeId) -> bool {
        let syntax::ast::Tree::Typed { tpt, .. } = *self.ast.tree(a) else { return false };
        matches!(*self.ast.tree(tpt), syntax::ast::Tree::Ident { name } if self.text(name.name) == "_*")
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

    /// Whether type `t` refers to a value parameter of the method type of binder `binder`, or of
    /// method `method`.
    fn mentions_params(&mut self, t: TypeId, binder: tasty::Binder, method: Option<SymbolId>) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            match *u.types().get(part) {
                Type::Param(b, _) if b == binder => found = true,
                Type::Sym(_, s) if u.symbol(s).kind == Kind::Param && method.is_some() => {
                    found |= u.symbol(s).owner == method;
                }
                _ => {}
            }
            None
        });
        found
    }

    /// Whether parameter type `p` is by-name or refers to a parameter.
    pub fn is_special_param(&mut self, p: TypeId) -> bool {
        let mut special = false;
        self.u.map_type(p, &mut |u, part| {
            special |= matches!(u.types().get(part), Type::ByName(_) | Type::Param(..));
            None
        });
        special
    }

    pub fn typed_new(&mut self, id: NodeId, tpt: NodeId, ctx: Ctx) -> TNodeId {
        let tpt = self.typed_type(tpt, ctx);
        let tpe = self.tpd.tpe(tpt);
        if let Type::Sym(_, c) = *self.u.types().get(tpe)
            && !self.u.type_params(c).is_empty()
        {
            return self.unsupported(id, "new of a generic class without type arguments");
        }
        let span = self.ast.span(id);
        self.tpd.node(T::New { tpt }, span, Some(tpe), None)
    }
}
