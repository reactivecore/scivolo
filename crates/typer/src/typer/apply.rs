// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Applications `f(args)` (dotty's `typedApply`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Attrs, List, NodeId, Tree};
use syntax::config::Features;
use tasty::{Kind, NameId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_apply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, fun_ctx: Ctx) -> TNodeId {
        if let Some(assign) = self.op_assign(id, fun, args, fun_ctx) {
            return assign;
        }
        let plain_ctx = fun_ctx.with_mode(fun_ctx.mode.without(Mode::EXTENSION_METHOD));
        let ctx = self.arg_ctx(fun, plain_ctx);
        let arg_ids = self.ast.items(args).to_vec();
        let proto = self.protos.len();
        let dual = self.infix_tuple_elems(id, &arg_ids);
        let using = self.ast.attrs(id).contains(Attrs::USING_APPLY);
        let fun_proto =
            super::protos::FunProto { args: arg_ids, ctx, typed: None, pt, deep: false, dual, using };
        self.protos.push(fun_proto);
        let f = self.typed(fun, Pt::Fun(proto), fun_ctx);
        let fun_ctx = plain_ctx;
        let once = !matches!(self.ast.tree(fun), Tree::Apply { .. });
        if self.protos[proto].args.is_empty() && once && self.drops_empty_args(f) {
            return f;
        }
        let f = match self.with_apply(f, Pt::Fun(proto), fun_ctx) {
            Ok(f) if self.is_pending_dynamic(f) => return self.dynamic_apply(id, f, proto, pt, fun_ctx),
            Ok(f) if self.is_error(f) => return self.error_tree(self.ast.span(id)),
            Ok(f) => f,
            Err("application of a non-method") => match self.non_method_applied((id, fun), f, proto, fun_ctx)
            {
                Ok(f) => f,
                Err(error) => return error,
            },
            Err(what) => return self.unsupported(id, what),
        };
        if let Some(m) = self.tpd.sym(self.tpd.meth_part(f))
            && let Err(what) = self.untuple_unless_unary(proto, &[m])
        {
            return self.unsupported(id, what);
        }
        let arg_ids = self.protos[proto].args.clone();
        let method = self.tpd.sym(f);
        let infix_meth = self.tpd.sym(self.tpd.meth_part(f));
        if self.is_invalid_infix(id, fun, infix_meth) {
            return self.unsupported(id, "alphanumeric infix operator under -source future");
        }
        if method.is_some_and(|m| self.u.full_name(m) == crate::defn::SUMMON_FROM) {
            return self.summon_from_applied(id, &arg_ids, pt, ctx);
        }
        let using = self.protos[proto].using;
        let f = match self.with_all_type_vars(f).and_then(|f| self.with_using_args(f, using, fun_ctx)) {
            Ok(g) if g != f => {
                let g = self.inlined_transparent(g, Pt::Fun(proto), fun_ctx).unwrap_or(g);
                self.with_apply(g, Pt::Fun(proto), fun_ctx)
            }
            other => other,
        };
        let f = match f {
            Ok(f) if self.is_pending_dynamic(f) => return self.dynamic_apply(id, f, proto, pt, fun_ctx),
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        let (outer, f) = self.lifted_fun(f);
        let pretyped = self.protos[proto].typed.take();
        let fun_tpe = self.tpd.tpe(f);
        let info = self.method_info(fun_tpe).or(Some(fun_tpe));
        let Some(Type::Method { binder, mut params, result, implicit, contextual }) =
            info.map(|i| self.u.ty(i).clone())
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
        let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
        let result = self.without_false_deps(result, binder, meth);
        let dependent = self.mentions_clause_params(result, (binder, &names), meth);
        if !dependent && let Err(what) = self.constrain_applied_result(meth, result, pt) {
            return self.unsupported(id, what);
        }
        let repeated = params.last().and_then(|&(_, p)| self.repeated_param_elem(p));
        let by_name_repeated =
            repeated.is_some() && params.last().is_some_and(|&(_, p)| self.u.by_name(p).is_some());
        if repeated.is_some() {
            params.pop();
        }
        let seq_arg = arg_ids
            .last()
            .copied()
            .filter(|&a| repeated.is_some() && arg_ids.len() == params.len() + 1 && self.is_sequence_arg(a));
        let named_seq = seq_arg.is_some_and(|a| {
            self.arg_name(a).is_some_and(|n| names.last().is_some_and(|&l| self.u.name(l) == n))
        });
        let unnamed = &arg_ids[..arg_ids.len() - usize::from(named_seq)];
        let aligned = match self.aligned_args(unnamed, &params, repeated.is_some()) {
            Ok(aligned) => aligned,
            Err(what) => return self.unsupported(id, what),
        };
        let formals: Vec<TypeId> =
            params.iter().map(|&(_, p)| p).chain(repeated).map(|p| self.u.by_name(p).unwrap_or(p)).collect();
        let in_order = aligned.is_none() && pretyped.is_none() && repeated.is_none();
        let clause = ((binder, &names[..]), meth);
        let applied = self.applied_params(f, meth);
        let param_dependent = match self.param_dependent(&formals, clause, &applied, in_order) {
            Ok(dependent) => dependent,
            Err(what) => return self.unsupported(id, what),
        };
        let spreads = repeated.is_some()
            && aligned.is_none()
            && arg_ids.iter().skip(params.len()).any(|&a| self.is_sequence_arg(a))
            && self.feature_enabled(Features::MULTI_SPREADS, "experimental.multiSpreads", ctx);
        let spread_at = |t: &Self, i: usize, a: NodeId| spreads && i >= params.len() && t.is_sequence_arg(a);
        let misplaced = (arg_ids.iter().enumerate())
            .any(|(i, &a)| self.is_sequence_arg(a) && Some(a) != seq_arg && !spread_at(self, i, a));
        if misplaced {
            return self.unsupported(id, "sequence argument");
        }
        if super::wildcard_members::is_bounds_like(self.u, result) {
            return self.unsupported(id, "result type of a wildcard argument");
        }
        let aligned_args = aligned.as_ref().map(|a| {
            self.typed_aligned_args((f, method), (&arg_ids, a, pretyped.as_deref()), (&params, &formals), ctx)
        });
        let (f, mut typed_args, lifted) = match aligned_args {
            _ if !outer.is_empty() && aligned.is_some() => {
                return self.unsupported(id, "named or default arguments after a lifted call");
            }
            Some(Err(what)) => return self.unsupported(id, what),
            Some(Ok(parts)) => parts,
            None => (f, Vec::new(), Vec::new()),
        };
        let plain = lifted.is_empty() && repeated.is_none();
        let result = match lifted.is_empty() {
            true => result,
            false => match self.lifted_result(f, meth) {
                Some(r) => r,
                None => return self.unsupported(id, "dependent result type"),
            },
        };
        let ctx = self.nullable_in_arg_ctx(f, ctx);
        let positional = if aligned.is_some() { &[][..] } else { &arg_ids[..] };
        let mut before_elems = None;
        for (i, &a) in positional.iter().enumerate().filter(|&(_, &a)| Some(a) != seq_arg) {
            before_elems = before_elems.or((i == params.len()).then(|| self.before_harmonic()));
            let mut formal = self.from_java_object(meth, formals[i.min(formals.len() - 1)]);
            if param_dependent {
                formal = self.dependent_formal(formal, clause, &typed_args);
            }
            let pre = pretyped.as_ref().and_then(|typed| typed[i]);
            if spread_at(self, i, a) {
                let spread = self.spread_arg(a, pre, formal, ctx);
                typed_args.push(spread);
                continue;
            }
            let by_name = params.get(i).is_some_and(|&(_, p)| self.u.by_name(p).is_some());
            let arg_ctx = if by_name { self.nullables.retract_mutables(self.u, ctx) } else { ctx };
            let arg = self.arg_against(a, pre, formal, arg_ctx);
            let arg = if by_name { self.by_name_uncaptured(arg) } else { arg };
            typed_args.push(self.by_name_flow_dropped(arg, i, meth));
        }
        let result = match dependent {
            true => self.dependent_result(result, (binder, &params), meth, &typed_args).filter(|_| plain),
            false => Some(result),
        };
        let Some(result) = result else { return self.unsupported(id, "dependent result type") };
        let erroneous = typed_args.iter().any(|&a| {
            let tpe = self.zonk(self.tpd.tpe(a));
            *self.u.ty(tpe) == Type::Error
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
                    let elem = self.normalized_tuple_type(elem);
                    let elem = self.java_object_elem(meth, elem);
                    let arg = self.seq_literal(&elems, elem, at);
                    if let T::Typed { expr, .. } = *self.tpd.tree(arg) {
                        if by_name_repeated {
                            self.type_seq_literal(expr);
                        }
                        self.note_spreads(expr, ctx);
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
        let partial = self.u.method(result).is_some();
        let tpe = match *self.u.ty(self.tpd.tpe(callee)) {
            _ if erroneous => self.u.intern_type(Type::Error),
            Type::Sym(Some(class), s)
                if self.u.symbol(s).name == "<init>" && self.u.symbol(s).kind == Kind::Def =>
            {
                if self.u.this_of(class).is_some() || partial {
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
        let widened = self.widen_enum_case(apply, infix_meth, pt);
        match self.converted_new_generic_array(widened).and_then(|t| self.lifted_partial(t, &lifted, pt)) {
            Ok(tree) => {
                let tree = self.wrap_defs(lifted, tree);
                let hoisted = !outer.is_empty();
                let tree = self.wrap_defs(outer, tree);
                if hoisted {
                    self.tpd.get_mut(tree).span = self.ast.span(id);
                }
                tree
            }
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

    /// The parameters of the value clauses of script method `method` that `f` applies.
    fn applied_params(&self, mut f: TNodeId, method: Option<tasty::SymbolId>) -> Vec<tasty::SymbolId> {
        let mut n = 0;
        while let T::Apply { fun, .. } | T::TypeApply { fun, .. } = *self.tpd.tree(f) {
            n += usize::from(matches!(self.tpd.tree(f), T::Apply { .. }));
            f = fun;
        }
        let clauses = method.and_then(|m| self.syms.param_clauses.get(&m));
        clauses.map_or_else(Vec::new, |c| c.iter().take(n).flatten().copied().collect())
    }
}
