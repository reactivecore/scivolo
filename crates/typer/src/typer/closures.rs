//! Function values `(x: T) => e` (dotty's `typedFunctionValue`): a closure is an anonymous
//! method and a `Closure` of it in a block, its parameter types from the declarations or the
//! expected function or single abstract method type.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{List, NodeId, TName, Tree};
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
        let (expected, sam) = match pt {
            Pt::Type(p) => match self.function_parts(p, n) {
                Some(parts) => (Some(parts), None),
                None if self.has_proto_wildcard(p) => {
                    return self
                        .unsupported(id, "function against an underspecified single abstract method type");
                }
                None => match self.sam_parts(p, n) {
                    Ok(Some(s)) => (Some((s.params, s.result)), Some(s.parent)),
                    Ok(None) => (None, None),
                    Err(what) => return self.unsupported(id, what),
                },
            },
            _ => (None, None),
        };
        let Some(class) = self.function_class(n) else {
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
                let formal = match self.fully_defined(formals[i]) {
                    Ok(t) if !self.has_wildcard(t) => t,
                    Ok(_) => return self.unsupported(p, "parameter type of a wildcard"),
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
            let def = T::ValDef { name, tpt, rhs: None };
            param_trees.push(self.tpd.node(def, self.ast.span(p), None, Some(sym)));
        }
        if missing {
            return self.error_tree(self.ast.span(id));
        }
        let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
        let params_syms: Vec<tasty::SymbolId> = entries.iter().map(|&(_, s)| s).collect();
        let level = self.levels.push(Some(ctx.level), Level::Scope(entries));
        let bctx = Ctx { owner: anonfun, level, ..ctx };
        let expected_result =
            expected.map(|(_, r)| r).filter(|&r| !matches!(self.u.types().get(r), Type::Wildcard(_)));
        let body_pt = expected_result.map_or(Pt::Any, Pt::Type);
        let body = self.typed(body, body_pt, bctx);
        let result = match expected_result {
            Some(r) => r,
            None => {
                let t = self.tpd.tpe(body);
                let t = self.fully_defined(t).unwrap_or(t);
                self.widen_singleton(t)
            }
        };
        let mut dependent = false;
        let body_tpe = self.tpd.tpe(body);
        for t in [result, body_tpe] {
            let t = self.zonk(t);
            let t = self.widen_singleton(t);
            self.u.map_type(t, &mut |u, part| {
                dependent |= matches!(*u.types().get(part), Type::Sym(_, s) if params_syms.contains(&s));
                None
            });
        }
        if dependent {
            return self.unsupported(id, "dependent function type");
        }
        let binder = self.syms.fresh_binder();
        let method_params = names.into_iter().zip(param_types.iter().copied()).collect();
        let info = self.u.intern_type(Type::Method {
            binder,
            params: method_params,
            result,
            implicit: false,
            contextual: false,
        });
        self.u.set_info(anonfun, info);
        self.syms.closure_syms.push(anonfun);
        let mut fun_args = param_types;
        fun_args.push(result);
        let fun_tpe = self.u.intern_type(Type::Applied(class, fun_args));
        self.closure_tree(id, anonfun, &param_trees, (result, body), sam.unwrap_or(fun_tpe), sam.is_some())
    }

    /// Whether `t` has a part left open by an expected type (dotty's `WildcardType`).
    fn has_proto_wildcard(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(u.types().get(part), Type::Wildcard(_));
            None
        });
        found
    }

    /// Whether `t` has a wildcard type argument or is one.
    fn has_wildcard(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(u.types().get(part), Type::Bounds(..) | Type::Wildcard(_));
            None
        });
        found
    }

    /// Function value `tree` typed as single abstract method type `pt` if it does not conform but
    /// fits the method (dotty's `toSAM`).
    pub fn to_sam(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let T::Block { stats, expr } = *self.tpd.tree(tree) else { return None };
        let T::Closure { env, meth, tpt: None } = *self.tpd.tree(expr) else { return None };
        let tpe = self.tpd.tpe(tree);
        if self.u.is_subtype(tpe, pt) {
            return None;
        }
        let n = (0..=22).find(|&n| self.function_parts(tpe, n).is_some())?;
        let parent = self.sam_parts(pt, n).ok()??.parent;
        let span = self.tpd.span(expr);
        let tpt = Some(self.type_tree(parent, span.start_pos(), false));
        let closure = self.tpd.node(T::Closure { env, meth, tpt }, span, Some(parent), None);
        let span = self.tpd.span(tree);
        Some(self.tpd.node(T::Block { stats, expr: closure }, span, Some(parent), None))
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
        let name = TName { name: self.name("$anonfun"), is_type: false };
        let clause = self.tpd.list(params);
        let paramss = self.tpd.clauses(&[clause]);
        let def = T::DefDef { name, paramss, tpt, rhs: Some(body) };
        let def = self.tpd.node(def, span, None, Some(anonfun));
        let meth_tpe = self.u.intern_type(Type::Sym(None, anonfun));
        let meth = self.tpd.node(T::Ident { name }, span.start_pos(), Some(meth_tpe), Some(anonfun));
        let env = self.tpd.list(&[]);
        let tpt = sam.then(|| self.type_tree(fun_tpe, span.start_pos(), false));
        let closure = self.tpd.node(T::Closure { env, meth, tpt }, span, Some(fun_tpe), None);
        let stats = self.tpd.list(&[def]);
        self.tpd.node(T::Block { stats, expr: closure }, span.to_synthetic(), Some(fun_tpe), None)
    }
}
