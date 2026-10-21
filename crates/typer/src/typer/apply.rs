// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Applications `f(args)` (dotty's `typedApply`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Attrs, List, NodeId, Tree};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn typed_apply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, fun_ctx: Ctx) -> TNodeId {
        if let Some(assign) = self.op_assign(id, fun, args, fun_ctx) {
            return assign;
        }
        let ctx = self.arg_ctx(fun, fun_ctx);
        let arg_ids = self.ast.items(args).to_vec();
        let proto = self.protos.len();
        let dual = self.infix_tuple_elems(id, &arg_ids);
        let using = self.ast.attrs(id).contains(Attrs::USING_APPLY);
        let fun_proto =
            super::overloads::FunProto { args: arg_ids, ctx, typed: None, pt, deep: false, dual, using };
        self.protos.push(fun_proto);
        let f = self.typed(fun, Pt::Fun(proto), fun_ctx);
        let f = match self.with_apply(f, Pt::Fun(proto), fun_ctx) {
            Ok(f) if self.is_pending_dynamic(f) => return self.dynamic_apply(id, f, proto, pt, fun_ctx),
            Ok(f) if self.is_error(f) => return self.error_tree(self.ast.span(id)),
            Ok(f) => f,
            Err(what @ "application of a non-method") => {
                let n = self.protos[proto].args.len();
                return match self.does_not_take_parameters(fun, f, n, fun_ctx) {
                    Some(error) => error,
                    None => self.unsupported(id, what),
                };
            }
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
            return self.summon_from_applied(id, &arg_ids, pt, ctx);
        }
        let using = self.protos[proto].using;
        let f = match self.with_type_vars(f).and_then(|f| self.with_using_args(f, using, fun_ctx)) {
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
        if let Err(what) = self.tuple_args_of_unary(proto, &params, ctx) {
            return self.unsupported(id, what);
        }
        let arg_ids = self.protos[proto].args.clone();
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
        let repeated = params.last().and_then(|&(_, p)| self.repeated_param_elem(p));
        let by_name_repeated = repeated.is_some()
            && params.last().is_some_and(|&(_, p)| matches!(self.u.types().get(p), Type::ByName(_)));
        if repeated.is_some() {
            params.pop();
        }
        let seq_arg = arg_ids
            .last()
            .copied()
            .filter(|&a| repeated.is_some() && arg_ids.len() == params.len() + 1 && self.is_sequence_arg(a));
        let named_seq = seq_arg.is_some_and(|a| self.arg_name(a).is_some_and(|n| Some(&n) == names.last()));
        let unnamed = &arg_ids[..arg_ids.len() - usize::from(named_seq)];
        let aligned = match self.aligned_args(unnamed, &params, repeated.is_some()) {
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
        let clause = ((binder, &names[..]), meth);
        let param_dependent = match self.param_dependent(&formals, clause, in_order, ctx) {
            Ok(dependent) => dependent,
            Err(what) => return self.unsupported(id, what),
        };
        let misplaced = arg_ids.iter().any(|&a| self.is_sequence_arg(a) && Some(a) != seq_arg);
        if misplaced {
            return self.unsupported(id, "sequence argument");
        }
        if let Type::Bounds(..) = self.u.types().get(result) {
            return self.unsupported(id, "result type of a wildcard argument");
        }
        let aligned_args = aligned.as_ref().map(|a| {
            self.typed_aligned_args((f, method), (&arg_ids, a, pretyped.as_deref()), (&params, &formals), ctx)
        });
        let (f, mut typed_args, lifted) = match aligned_args {
            Some(Err(what)) => return self.unsupported(id, what),
            Some(Ok(parts)) => parts,
            None => (f, Vec::new(), Vec::new()),
        };
        let plain = lifted.is_empty() && aligned.is_none() && repeated.is_none();
        let result = match lifted.is_empty() {
            true => result,
            false => match self.lifted_result(f, meth) {
                Some(r) => r,
                None => return self.unsupported(id, "dependent result type"),
            },
        };
        let positional = if aligned.is_some() { &[][..] } else { &arg_ids[..] };
        let mut before_elems = None;
        for (i, &a) in positional.iter().enumerate().filter(|&(_, &a)| Some(a) != seq_arg) {
            before_elems = before_elems.or((i == params.len()).then(|| self.before_harmonic()));
            let mut formal = self.from_java_object(meth, formals[i.min(formals.len() - 1)]);
            if param_dependent {
                formal = self.dependent_formal(formal, clause, &typed_args);
            }
            let pre = pretyped.as_ref().and_then(|typed| typed[i]);
            typed_args.push(self.arg_against(a, pre, formal, ctx));
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
                    match pretyped.as_ref().and_then(|typed| typed[arg_ids.len() - 1]) {
                        Some(typed) => match self.repeated_elem(self.tpd.tpe(typed)) {
                            Some(e) if self.conforms(e, elem) => typed,
                            _ => return self.unsupported(id, "sequence argument of an overload"),
                        },
                        None => self.named_sequence_arg(a, elem, ctx),
                    }
                }
                None => {
                    let mut elems = typed_args.split_off(params.len());
                    if self.is_open(result) {
                        elems = match self.harmonic(elems, Pt::Type(elem), before_elems, ctx) {
                            Ok(h) => h,
                            Err(what) => return self.unsupported(id, what),
                        };
                    }
                    let before = typed_args.last().copied().unwrap_or(f);
                    let at = self.tpd.span(before).end_pos();
                    let arg = self.seq_literal(&elems, elem, at);
                    if let T::Typed { expr, .. } = *self.tpd.tree(arg)
                        && by_name_repeated
                    {
                        self.type_seq_literal(expr);
                    }
                    arg
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
        let widened = self.widen_enum_case(apply, method, pt);
        match self.converted_new_generic_array(widened) {
            Ok(tree) => self.wrap_defs(lifted, tree),
            Err(what) => self.unsupported(id, what),
        }
    }

    /// Whether argument `a` is a sequence passed to a repeated parameter, `xs*` or `p = xs*`.
    pub fn is_sequence_arg(&self, a: NodeId) -> bool {
        match *self.ast.tree(a) {
            Tree::NamedArg { arg, .. } => self.is_sequence_arg(arg),
            Tree::Typed { tpt, .. } => {
                matches!(*self.ast.tree(tpt), Tree::Ident { name } if self.text(name.name) == "_*")
            }
            _ => false,
        }
    }

    /// Sequence argument `a`, named or not, passed to a repeated parameter of element type `elem`.
    fn named_sequence_arg(&mut self, a: NodeId, elem: TypeId, ctx: Ctx) -> TNodeId {
        let Tree::NamedArg { name, arg } = *self.ast.tree(a) else {
            return self.sequence_arg(a, Some(elem), ctx);
        };
        let arg = self.sequence_arg(arg, Some(elem), ctx);
        let tpe = self.tpd.get(arg).tpe;
        self.tpd.node(T::NamedArg { name, arg }, self.ast.span(a), tpe, None)
    }
}
