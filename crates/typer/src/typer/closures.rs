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
        let dependent_pt = match pt {
            Pt::Type(p) => self
                .dependent_function_parts(p, n, contextual)
                .or_else(|| self.erased_function_parts(p, n, contextual)),
            _ => None,
        };
        let (expected, sam) = match &dependent_pt {
            Some((parts, _)) => (Some(parts.clone()), None),
            None => match self.expected_function_parts(id, pt, n, contextual) {
                Ok(parts) => parts,
                Err(refused) => return refused,
            },
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
        let (mut missing, mut any_erased) = (false, false);
        for (i, &p) in params.iter().enumerate() {
            let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else {
                return self.unsupported(p, "function parameter");
            };
            let tpt = if !self.is_untyped_param(p) {
                self.typed_type(tpt, ctx)
            } else if let Some((formals, _)) = &expected {
                let formal = self.zonk(formals[i]);
                let formal = self.without_erased_param(formal);
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
            let mut flags = self.ast.modifiers(mods).flags.or(f::TERM_PARAM).to_term_flags();
            if self.derives_erased_class(tpe) {
                flags = flags.or(f::ERASED);
            }
            any_erased |= flags.is(f::ERASED);
            let sym = self.syms.define(self.u, &text, Kind::Param, anonfun, flags);
            self.u.set_info(sym, tpe);
            self.syms.closure_syms.push(sym);
            entries.push((if self.hidden_params.contains(&p) { format!("`{text}`") } else { text }, sym));
            param_types.push(tpe);
            let span = self.ast.span(p);
            param_trees.push(self.build().val_def(sym, tpt, None, span));
        }
        if missing {
            return self.error_tree(self.ast.span(id));
        }
        let names: Vec<String> = entries.iter().map(|&(_, s)| self.u.symbol(s).name.clone()).collect();
        let params_syms: Vec<tasty::SymbolId> = entries.iter().map(|&(_, s)| s).collect();
        let level = self.levels.push(Some(ctx.level), Level::Scope(entries));
        let bctx = Ctx { owner: anonfun, level, ..ctx };
        let expected_result = expected.map(|(_, r)| r).and_then(|r| match *self.u.types().get(r) {
            Type::Wildcard(_) => None,
            Type::Bounds(_, hi) => Some(hi),
            _ => Some(r),
        });
        let refs: Vec<TypeId> = params_syms.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))).collect();
        let poly = self.poly_closures.get(&id).cloned();
        let bound = poly.as_ref().and_then(Option::as_ref).or(dependent_pt.as_ref().map(|(_, b)| b));
        let expected_result = match (&sam, bound) {
            (Some((_, (sam_binder, sam_names, method))), _) => expected_result
                .map(|r| self.applied_result(r, (*sam_binder, sam_names), Some(*method), &refs)),
            (None, Some((binder, names))) => {
                expected_result.map(|r| self.applied_result(r, (*binder, names), None, &refs))
            }
            (None, None) => expected_result,
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
        let checked = match expected_result {
            Some(_) if self.var_bounds_refer_to(result, &params_syms) => {
                return self.unsupported(id, "local type escaping through a type variable");
            }
            Some(_) => vec![result],
            None => vec![result, self.tpd.tpe(body)],
        };
        let binder = self.syms.fresh_binder();
        let dependent_tpe = match sam.is_none() && poly.is_none() {
            true => match self.closure_dependent_type((class, contextual), &params_syms, binder, &checked) {
                Ok(t) => t,
                Err(what) => return self.unsupported(id, what),
            },
            false => None,
        };
        let info_result = self.u.map_type(result, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => {
                let i = params_syms.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        });
        if any_erased && (sam.is_some() || poly.is_some()) {
            return self.unsupported(id, "erased parameter of a closure of a single abstract method type");
        }
        let info_params: Vec<TypeId> = params_syms.iter().map(|&p| self.method_param_type(p)).collect();
        let method_params = names.into_iter().zip(info_params).collect();
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
        let erased_tpe =
            any_erased.then(|| self.erased_function_of(&params_syms, result, contextual)).flatten();
        let fun_tpe = erased_tpe
            .or(dependent_tpe)
            .unwrap_or_else(|| self.u.intern_type(Type::Applied(class, fun_args)));
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
        let tpe = if sam { fun_tpe } else { self.simplified(fun_tpe) };
        let closure = self.tpd.node(T::Closure { env, meth, tpt }, span, Some(tpe), None);
        let stats = self.tpd.list(&[def]);
        self.build().block(stats, closure, span.to_synthetic())
    }
}
