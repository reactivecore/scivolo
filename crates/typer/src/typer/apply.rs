//! Applications `f(args)` (dotty's `typedApply`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_apply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, ctx: Ctx) -> TNodeId {
        if let Some(assign) = self.op_assign(id, fun, args, ctx) {
            return assign;
        }
        let arg_ids = self.ast.items(args).to_vec();
        let proto = self.protos.len();
        self.protos.push(super::overloads::FunProto { args: arg_ids.clone(), ctx, typed: None });
        let f = self.typed(fun, Pt::Fun(proto), ctx);
        let f = match self.with_apply(f, Pt::Fun(proto), ctx) {
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
        let Some(Type::Method { binder, mut params, result, implicit, contextual }) =
            info.map(|i| self.u.types().get(i).clone())
        else {
            return self.unsupported(id, "application of a non-method");
        };
        let using = self.ast.attrs(id).contains(syntax::ast::Attrs::USING_APPLY);
        if contextual && !using || using && !implicit {
            return self.unsupported(id, "arguments not matching the clause's using");
        }
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
        let aligned = match self.aligned_args(&arg_ids, &params, repeated.is_some(), pretyped.is_some()) {
            Ok(aligned) => aligned,
            Err(what) => return self.unsupported(id, what),
        };
        let formals: Vec<TypeId> = params
            .iter()
            .map(|&(_, p)| p)
            .chain(repeated)
            .map(|p| match *self.u.types().get(p) {
                Type::ByName(r) => r,
                _ => p,
            })
            .collect();
        if formals.iter().any(|&p| {
            self.is_special_param(p)
                || self.mentions_params(p, binder, method)
                || self.mentions_foreign_private_type(p, ctx)
        }) {
            return self.unsupported(id, "dependent parameter or private type");
        }
        let seq_arg = arg_ids
            .last()
            .copied()
            .filter(|&a| repeated.is_some() && arg_ids.len() == params.len() + 1 && self.is_sequence_arg(a));
        let misplaced = arg_ids.iter().any(|&a| self.is_sequence_arg(a) && Some(a) != seq_arg);
        if misplaced || seq_arg.is_some() && pretyped.is_some() {
            return self.unsupported(id, "sequence argument");
        }
        if let Type::Bounds(..) = self.u.types().get(result) {
            return self.unsupported(id, "result type of a wildcard argument");
        }
        let mut typed_args = match aligned
            .as_ref()
            .map(|a| self.typed_aligned_args(f, method, (&arg_ids, a), &formals, ctx))
        {
            Some(Err(what)) => return self.unsupported(id, what),
            Some(Ok(args)) => args,
            None => Vec::new(),
        };
        let positional = if aligned.is_some() { &[][..] } else { &arg_ids[..] };
        for (i, &a) in positional.iter().enumerate().filter(|&(_, &a)| Some(a) != seq_arg) {
            let formal = formals[i.min(formals.len() - 1)];
            typed_args.push(match &pretyped {
                Some(typed) => {
                    let arg = self.to_sam(typed[i], formal).unwrap_or(typed[i]);
                    self.adapt(arg, Pt::Type(formal), ctx)
                }
                None => self.typed(a, Pt::Type(formal), ctx),
            });
        }
        let span = self.ast.span(id);
        if let Some(elem) = repeated {
            let arg = match seq_arg {
                Some(a) => self.sequence_arg(a, elem, ctx),
                None => {
                    let elems = typed_args.split_off(params.len());
                    let before = typed_args.last().copied().unwrap_or(f);
                    let at = self.tpd.span(before).end_pos();
                    self.seq_literal(&elems, elem, at)
                }
            };
            typed_args.push(arg);
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
            _ if partial => result,
            _ => self.simplified(result),
        };
        let args = self.tpd.list(&typed_args);
        let apply = self.tpd.node(T::Apply { fun: f, args }, span, Some(tpe), None);
        self.fold_apply(apply);
        match self.widen_enum_case(apply, method, pt) {
            Ok(tree) => tree,
            Err(what) => self.unsupported(id, what),
        }
    }

    /// `f`, or `f.apply` if `f` is not a method (dotty's `tryInsertApplyOrImplicit`), the
    /// alternative chosen by `pt`, arguments or type arguments.
    pub fn with_apply(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. } | Type::Error) {
            return Ok(f);
        }
        let tpe = self.widen_if_unstable(tpe);
        let Some(found) = self.members_found(tpe, "apply", false) else {
            let proxy = if matches!(pt, Pt::Fun(_)) { self.proxy_companion(f, pt, ctx) } else { None };
            return proxy.ok_or("application of a non-method");
        };
        let sym = self.choose(&found.syms, Some(tpe), pt)?;
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
}
