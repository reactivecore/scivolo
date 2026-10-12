//! Applications `f(args)` (dotty's `typedApply`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn typed_apply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, ctx: Ctx) -> TNodeId {
        if let Some(assign) = self.op_assign(id, fun, args, ctx) {
            return assign;
        }
        let arg_ids = self.ast.items(args).to_vec();
        let proto = self.protos.len();
        let applied = matches!(pt, Pt::Fun(_) | Pt::PolyFun);
        let result = if let Pt::Type(p) = pt { Some(p) } else { None };
        self.protos.push(super::overloads::FunProto {
            args: arg_ids.clone(),
            ctx,
            typed: None,
            applied,
            result,
        });
        let f = self.typed(fun, Pt::Fun(proto), ctx);
        let f = match self.with_apply(f, Pt::Fun(proto), ctx) {
            Ok(f) if self.is_error(f) => return self.error_tree(self.ast.span(id)),
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        let method = self.tpd.sym(f);
        if method.is_some_and(|m| self.u.full_name(m) == "scala.compiletime.package$package$.summonFrom") {
            return match arg_ids[..] {
                [arg] => match *self.ast.tree(arg) {
                    syntax::ast::Tree::Match { selector: None, cases, .. } => {
                        self.typed_summon_from(id, cases, pt, ctx)
                    }
                    _ => {
                        self.unsupported(id, "argument to summonFrom that is not a pattern matching closure")
                    }
                },
                _ => self.unsupported(id, "summonFrom"),
            };
        }
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
        if implicit && !using && params.first().is_some_and(|(n, _)| n.starts_with("evidence$")) {
            return self.unsupported(id, "explicit arguments to context bound parameters");
        }
        if self.mentions_params(result, binder, method) {
            return self.unsupported(id, "dependent result type");
        }
        let mut value = result;
        let defined_pt = matches!(pt, Pt::Type(p) if self.is_fully_defined(p) && self.u.class_symbol(p) != Some(self.defn.unit));
        while let Type::Method { result: r, implicit: true, .. } = *self.u.types().get(value)
            && defined_pt
        {
            value = r;
        }
        if let Pt::Type(p) = pt
            && !matches!(self.u.types().get(value), Type::Method { .. } | Type::Poly { .. })
        {
            let p = self.result_pt(self.tpd.sym(self.tpd.meth_part(f)), p);
            self.conforms(value, p);
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
        let aligned_args = aligned
            .as_ref()
            .map(|a| self.typed_aligned_args((f, method), (&arg_ids, a), (&params, &formals), ctx));
        let (f, mut typed_args, lifted) = match aligned_args {
            Some(Err(what)) => return self.unsupported(id, what),
            Some(Ok(parts)) => parts,
            None => (f, Vec::new(), Vec::new()),
        };
        let result = if lifted.is_empty() { result } else { self.result_of(f).unwrap_or(result) };
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
        let erroneous = typed_args.iter().any(|&a| {
            let tpe = self.zonk(self.tpd.tpe(a));
            *self.u.types().get(tpe) == Type::Error
        });
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
            && let Err(what) = self.check_can_equal(f, self.tpd.tpe(arg), self.ast.span(id), ctx)
        {
            return self.unsupported(id, what);
        }
        let callee = self.tpd.meth_part(f);
        let partial = matches!(self.u.types().get(result), Type::Method { .. });
        let tpe = match self.u.types().get(self.tpd.tpe(callee)).clone() {
            _ if erroneous => self.u.intern_type(Type::Error),
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
        let apply = self.tpd.node(T::Apply { fun: f, args }, self.ast.span(id), Some(tpe), None);
        self.fold_apply(apply);
        match self.widen_enum_case(apply, method, pt).and_then(|t| self.converted_new_generic_array(t)) {
            Ok(tree) => self.wrap_defs(lifted, tree),
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
        let sym = match self.choose(&found.syms, Some(tpe), pt) {
            Err(_) if self.unusable_for_inference(pt) => return Ok(self.error_tree(self.tpd.span(f))),
            chosen => chosen?,
        };
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
}
