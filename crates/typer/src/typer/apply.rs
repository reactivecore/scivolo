//! Applications `f(args)` (dotty's `typedApply`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn typed_apply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, fun_ctx: Ctx) -> TNodeId {
        if let Some(assign) = self.op_assign(id, fun, args, fun_ctx) {
            return assign;
        }
        let ctx = self.arg_ctx(fun, fun_ctx);
        let arg_ids = self.ast.items(args).to_vec();
        let proto = self.protos.len();
        let applied = matches!(pt, Pt::Fun(_) | Pt::PolyFun);
        let result = if let Pt::Type(p) = pt { Some(p) } else { None };
        let dual = self.infix_tuple_elems(id, &arg_ids);
        self.protos.push(super::overloads::FunProto {
            args: arg_ids,
            ctx,
            typed: None,
            applied,
            result,
            dual,
        });
        let f = self.typed(fun, Pt::Fun(proto), fun_ctx);
        let f = match self.with_apply(f, Pt::Fun(proto), fun_ctx) {
            Ok(f) if self.is_error(f) => return self.error_tree(self.ast.span(id)),
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        if let Some(m) = self.tpd.sym(self.tpd.meth_part(f))
            && let Err(what) = self.untuple_unless_unary(proto, &[m])
        {
            return self.unsupported(id, what);
        }
        let arg_ids = self.protos[proto].args.clone();
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
        let f = match self.with_type_vars(f).and_then(|f| self.with_using_args(f, id, fun_ctx)) {
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
        if let Err(what) = self.check_clause_args((id, &arg_ids), &params, implicit, contextual) {
            return self.unsupported(id, what);
        }
        let meth = self.tpd.sym(self.tpd.meth_part(f));
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let result = self.without_false_deps(result, binder, meth);
        let dependent = self.mentions_clause_params(result, (binder, &names), meth);
        if !dependent && let Err(what) = self.constrain_applied_result(meth, result, pt) {
            return self.unsupported(id, what);
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
        let in_order = aligned.is_none() && pretyped.is_none() && repeated.is_none();
        let mut param_dependent = false;
        for &p in &formals {
            let own = self.mentions_clause_params(p, (binder, &names), meth);
            param_dependent |= own;
            if own && !in_order
                || !own && (self.is_special_param(p) || self.mentions_params(p, binder, meth))
                || self.mentions_foreign_private_type(p, ctx)
            {
                return self.unsupported(id, "dependent parameter or private type");
            }
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
        let plain = lifted.is_empty() && aligned.is_none() && repeated.is_none();
        let result = if lifted.is_empty() { result } else { self.result_of(f).unwrap_or(result) };
        let positional = if aligned.is_some() { &[][..] } else { &arg_ids[..] };
        for (i, &a) in positional.iter().enumerate().filter(|&(_, &a)| Some(a) != seq_arg) {
            let mut formal = self.from_java_object(meth, formals[i.min(formals.len() - 1)]);
            if param_dependent {
                let before: Vec<TypeId> = typed_args.iter().map(|&a| self.tpd.tpe(a)).collect();
                formal = self.applied_result(formal, (binder, &names), meth, &before);
                self.complete_type_members(formal);
            }
            typed_args.push(match pretyped.as_ref().and_then(|typed| typed[i]) {
                Some(typed) => {
                    let arg = self.to_sam(typed, formal).unwrap_or(typed);
                    self.adapt(arg, Pt::Type(formal), ctx)
                }
                None => self.typed(a, Pt::Type(formal), ctx),
            });
        }
        let result = match dependent {
            true => self.dependent_result(result, (binder, &params), meth, &typed_args).filter(|_| plain),
            false => Some(result),
        };
        let Some(result) = result else { return self.unsupported(id, "dependent result type") };
        let erroneous = typed_args.iter().any(|&a| {
            let tpe = self.zonk(self.tpd.tpe(a));
            *self.u.types().get(tpe) == Type::Error
        });
        if let Some(elem) = repeated {
            let arg = match seq_arg {
                Some(a) => {
                    let elem = self.from_java_object(meth, elem);
                    self.sequence_arg(a, elem, ctx)
                }
                None => {
                    let elems = typed_args.split_off(params.len());
                    let types: Vec<TypeId> = elems.iter().map(|&e| self.tpd.tpe(e)).collect();
                    if !self.is_fully_defined(elem) && self.harmonizes(&types) {
                        return self.unsupported(id, "numeric harmonization");
                    }
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
                if let Type::This(c) = *self.u.types().get(class) {
                    if self.u.class_symbol(c).is_some_and(|c| !self.u.type_params(c).is_empty()) {
                        return self.unsupported(id, "secondary constructor call of a polymorphic class");
                    }
                    result
                } else if partial {
                    result
                } else {
                    self.constructor_result(s, class, result)
                }
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

    /// Whether argument `a` is a sequence passed to a repeated parameter, `xs*`.
    fn is_sequence_arg(&self, a: NodeId) -> bool {
        let syntax::ast::Tree::Typed { tpt, .. } = *self.ast.tree(a) else { return false };
        matches!(*self.ast.tree(tpt), syntax::ast::Tree::Ident { name } if self.text(name.name) == "_*")
    }
}
