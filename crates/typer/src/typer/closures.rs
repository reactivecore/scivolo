// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Function values `(x: T) => e` (dotty's `typedFunctionValue`): a closure is an anonymous
//! method and a `Closure` of it in a block, its parameter types from the declarations or the
//! expected function or single abstract method type.

use super::{Pt, Typer, Widen};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{List, NodeId, Tree};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Function value `(params) => body` against the expected type `pt`.
    pub fn typed_function(&mut self, id: NodeId, args: List, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let params = self.ast.items(args).to_vec();
        let n = params.len();
        let pt = self.instantiated_pt(id, (&params, body), pt);
        if let Some(untupled) = self.untupled_function(id, args, body, pt, ctx) {
            return untupled;
        }
        let contextual = params.first().is_some_and(|&p| self.is_given_param(p));
        let (expected, sam) = match self.expected_function_parts(id, pt, n, contextual) {
            Ok(parts) => parts,
            Err(refused) => return refused,
        };
        let class = if contextual { self.context_function_class(n) } else { self.function_class(n) };
        let Some(class) = class else {
            return self.unsupported(id, "function of that many parameters");
        };
        let anonfun = self.syms.define(
            self.u,
            "$anonfun",
            Kind::Def,
            ctx.owner,
            f::METHOD.or(f::SYNTHETIC).or(f::ARTIFACT),
        );
        let mut entries = Vec::new();
        let mut param_trees = Vec::new();
        let mut param_types = Vec::new();
        let mut missing = false;
        for (i, &p) in params.iter().enumerate() {
            let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else {
                return self.unsupported(p, "function parameter");
            };
            let tpt = if !self.is_untyped_param(p) {
                self.typed_type(tpt, ctx)
            } else if let Some((formals, _)) = &expected {
                let formal = self.zonk(formals[i]);
                let lower = match *self.u.types().get(formal) {
                    Type::Bounds(lo, _) => Some(lo),
                    _ => None,
                };
                let nothing = Some(self.u.builtins().nothing);
                if lower.is_some_and(|lo| self.u.class_symbol(lo) == nothing) {
                    return self.unsupported(p, "parameter type inferred from the target of a wildcard");
                }
                let formal = lower.unwrap_or(formal);
                if contextual && !self.is_fully_defined(formal) {
                    return self.unsupported(p, "context closure parameter of an open type");
                }
                let formal = match self.fully_defined(formal) {
                    Ok(t) if !self.has_proto_wildcard(t) => t,
                    Ok(_) => return self.unsupported(p, "parameter type left open by the expected type"),
                    Err(what) => return self.unsupported(p, what),
                };
                self.type_tree(formal, self.ast.span(p).end_pos(), true)
            } else {
                let Some(error) = self.missing_param_type((id, p), (&params, body), pt, ctx) else {
                    return self.unsupported(p, "function parameter without a type");
                };
                missing = true;
                error
            };
            let tpe = self.tpd.tpe(tpt);
            let text = self.text(name.name).to_owned();
            let flags = self.ast.modifiers(mods).flags.or(f::TERM_PARAM).to_term_flags();
            let sym = self.syms.define(self.u, &text, Kind::Param, anonfun, flags);
            self.u.set_info(sym, tpe);
            self.syms.closure_syms.push(sym);
            entries.push((text, sym));
            param_types.push(tpe);
            let span = self.ast.span(p);
            param_trees.push(self.build().val_def(sym, tpt, None, span));
        }
        if missing {
            return self.error_tree(self.ast.span(id));
        }
        let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
        let params_syms: Vec<tasty::SymbolId> = entries.iter().map(|&(_, s)| s).collect();
        let level = self.levels.push(Some(ctx.level), Level::Scope(entries));
        let bctx = Ctx { owner: anonfun, level, ..ctx };
        let expected_result = expected.map(|(_, r)| r).and_then(|r| match *self.u.types().get(r) {
            Type::Wildcard(_) => None,
            Type::Bounds(_, hi) => Some(hi),
            _ => Some(r),
        });
        let expected_result = match &sam {
            Some((_, (sam_binder, sam_names, method))) => expected_result.map(|r| {
                let refs: Vec<TypeId> =
                    params_syms.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))).collect();
                self.applied_result(r, (*sam_binder, sam_names), Some(*method), &refs)
            }),
            None => expected_result,
        };
        let body_pt = expected_result.map_or(Pt::Any, Pt::Type);
        let body = self.typed(body, body_pt, bctx);
        let result = match expected_result {
            Some(r) => r,
            None => {
                let t = self.tpd.tpe(body);
                let t = self.fully_defined(t).unwrap_or(t);
                self.widen_inferred(t, tasty::ANY, Widen::Unions).unwrap_or_else(|what| {
                    self.unsupported_at(self.tpd.span(body), what);
                    t
                })
            }
        };
        let mut dependent = false;
        let body_tpe = self.tpd.tpe(body);
        for t in [result, body_tpe] {
            let t = self.zonk(t);
            let t = self.widen_singletons(t, false);
            self.u.map_type(t, &mut |u, part| {
                dependent |= matches!(*u.types().get(part), Type::Sym(_, s) if params_syms.contains(&s));
                None
            });
        }
        if dependent && sam.is_none() {
            return self.unsupported(id, "dependent function type");
        }
        let binder = self.syms.fresh_binder();
        let info_result = self.u.map_type(result, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => {
                let i = params_syms.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        });
        let method_params = names.into_iter().zip(param_types.iter().copied()).collect();
        let info = self.u.intern_type(Type::Method {
            binder,
            params: method_params,
            result: info_result,
            implicit: contextual,
            contextual,
        });
        self.u.set_info(anonfun, info);
        self.syms.closure_syms.push(anonfun);
        let sam = match sam.map(|(p, _)| self.closure_parent(p, &param_types, result)).transpose() {
            Ok(sam) => sam,
            Err(what) => return self.unsupported(id, what),
        };
        let mut fun_args = param_types;
        fun_args.push(result);
        let fun_tpe = self.u.intern_type(Type::Applied(class, fun_args));
        self.closure_tree(id, anonfun, &param_trees, (result, body), sam.unwrap_or(fun_tpe), sam.is_some())
    }

    /// `{ def $anonfun(params): result = body; closure($anonfun) }` of type `fun_tpe` at the span
    /// of function `id`, the closure given that type if `sam`.
    fn closure_tree(
        &mut self,
        id: NodeId,
        anonfun: tasty::SymbolId,
        params: &[TNodeId],
        (result, body): (TypeId, TNodeId),
        fun_tpe: TypeId,
        sam: bool,
    ) -> TNodeId {
        let span = self.ast.span(id);
        let result_at = match params.last() {
            Some(&p) => self.tpd.span(p).end_pos(),
            None => self.tpd.span(body).start_pos(),
        };
        let tpt = self.type_tree(result, result_at, true);
        let def = self.build().def_def(anonfun, &[params], tpt, Some(body), span);
        let meth_tpe = self.u.intern_type(Type::Sym(None, anonfun));
        let meth = self.build().ident(anonfun, meth_tpe, span.start_pos());
        let env = self.tpd.list(&[]);
        let tpt = sam.then(|| self.type_tree(fun_tpe, span.start_pos(), false));
        let closure = self.tpd.node(T::Closure { env, meth, tpt }, span, Some(fun_tpe), None);
        let stats = self.tpd.list(&[def]);
        self.build().block(stats, closure, span.to_synthetic())
    }
}
