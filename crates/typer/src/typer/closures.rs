//! Function values `(x: T) => e` and function types `A => B` (dotty's `typedFunctionValue`,
//! `typedFunctionType`): a closure is an anonymous method and a `Closure` of it in a block,
//! its parameter types from the declarations or the expected function type.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{List, NodeId, TName, Tree};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// The class `scala.FunctionN` of `n` parameters.
    fn function_class(&mut self, n: usize) -> Option<TypeId> {
        let c = self.resolve_full_name(&format!("scala.Function{n}"), true)?;
        Some(crate::defn::type_ref(self.u, c))
    }

    /// The parameter and result types of `t` if it is a `FunctionN` of `n` parameters.
    fn function_parts(&mut self, t: TypeId, n: usize) -> Option<(Vec<TypeId>, TypeId)> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let class = self.u.class_symbol(tycon)?;
        (self.u.full_name(class) == format!("scala.Function{n}") && args.len() == n + 1)
            .then(|| (args[..n].to_vec(), args[n]))
    }

    /// Type `t` as dotty's messages show it: function types as `(A, B) => R`.
    pub fn show_type(&mut self, t: TypeId) -> String {
        let t = self.zonk(t);
        if let Type::Applied(tycon, args) = self.u.types().get(t).clone()
            && let Some(class) = self.u.class_symbol(tycon)
            && self.u.full_name(class) == format!("scala.Function{}", args.len() - 1)
        {
            let (params, result) = args.split_at(args.len() - 1);
            let params: Vec<String> = params.iter().map(|&p| self.show_type(p)).collect();
            let result = self.show_type(result[0]);
            return match params.as_slice() {
                [p] if !p.contains("=>") => format!("{p} => {result}"),
                _ => format!("({}) => {result}", params.join(", ")),
            };
        }
        self.u.show(t)
    }

    /// Function type `(args) => body` as `FunctionN[args, body]`.
    pub fn typed_function_type(&mut self, id: NodeId, args: List, body: NodeId, ctx: Ctx) -> TNodeId {
        let arg_ids = self.ast.items(args).to_vec();
        let Some(class) = self.function_class(arg_ids.len()) else {
            return self.unsupported(id, "function type of that many parameters");
        };
        let mut typed: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
        typed.push(self.typed_type(body, ctx));
        if typed.iter().any(|&t| matches!(self.tpd.tree(t), T::ByNameTypeTree { .. })) {
            return self.unsupported(id, "by-name function parameter");
        }
        let span = self.ast.span(id);
        let first = arg_ids.first().copied().unwrap_or(body);
        let tycon = self.type_tree(class, self.ast.span(first).start_pos(), false);
        let arg_types: Vec<TypeId> = typed.iter().map(|&t| self.tpd.tpe(t)).collect();
        let tpe = self.u.intern_type(Type::Applied(class, arg_types));
        let args = self.tpd.list(&typed);
        self.tpd.node(T::AppliedTypeTree { tpt: tycon, args }, span, Some(tpe), None)
    }

    /// Function value `(params) => body` against the expected type `pt`.
    pub fn typed_function(&mut self, id: NodeId, args: List, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let params = self.ast.items(args).to_vec();
        let n = params.len();
        let expected = match pt {
            Pt::Type(p) => self.function_parts(p, n),
            _ => None,
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
        for (i, &p) in params.iter().enumerate() {
            let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else {
                return self.unsupported(p, "function parameter");
            };
            let declared = !matches!(self.ast.tree(tpt), Tree::TypeTree);
            let tpt = if declared {
                self.typed_type(tpt, ctx)
            } else {
                let Some(formal) = expected.as_ref().map(|(ps, _)| ps[i]) else {
                    return self.unsupported(p, "function parameter without a type");
                };
                let formal = match self.fully_defined(formal) {
                    Ok(t) => t,
                    Err(what) => return self.unsupported(p, what),
                };
                self.type_tree(formal, self.ast.span(p).end_pos(), true)
            };
            let tpe = self.tpd.tpe(tpt);
            let text = self.text(name.name).to_owned();
            let flags = self.ast.modifiers(mods).flags.or(f::TERM_PARAM).to_term_flags();
            let sym = self.syms.define(self.u, &text, Kind::Param, anonfun, flags);
            self.u.set_info(sym, tpe);
            self.syms.closure_syms.push(sym);
            entries.push((text, sym));
            param_types.push(tpe);
            param_trees.push(self.tpd.node(
                T::ValDef { name, tpt, rhs: None },
                self.ast.span(p),
                None,
                Some(sym),
            ));
        }
        let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
        let level = self.levels.push(Some(ctx.level), Level::Scope(entries));
        let bctx = Ctx { owner: anonfun, level, ..ctx };
        let body_pt = expected.as_ref().map_or(Pt::Any, |&(_, r)| Pt::Type(r));
        let body = self.typed(body, body_pt, bctx);
        let result = match expected {
            Some((_, r)) => r,
            None => {
                let t = self.tpd.tpe(body);
                let t = self.fully_defined(t).unwrap_or(t);
                self.widen_singleton(t)
            }
        };
        let params_syms: Vec<tasty::SymbolId> =
            self.syms.closure_syms[self.syms.closure_syms.len() - n..].to_vec();
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
        let info =
            self.u.intern_type(Type::Method { binder, params: method_params, result, implicit: false });
        self.u.set_info(anonfun, info);
        self.syms.closure_syms.push(anonfun);
        let mut fun_args = param_types;
        fun_args.push(result);
        let fun_tpe = self.u.intern_type(Type::Applied(class, fun_args));
        self.closure_tree(id, anonfun, &param_trees, result, body, fun_tpe)
    }

    /// `{ def $anonfun(params): result = body; closure($anonfun) }` of type `fun_tpe` at the span
    /// of function `id`.
    fn closure_tree(
        &mut self,
        id: NodeId,
        anonfun: tasty::SymbolId,
        params: &[TNodeId],
        result: TypeId,
        body: TNodeId,
        fun_tpe: TypeId,
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
        let closure = self.tpd.node(T::Closure { env, meth, tpt: None }, span, Some(fun_tpe), None);
        let stats = self.tpd.list(&[def]);
        self.tpd.node(T::Block { stats, expr: closure }, span.to_synthetic(), Some(fun_tpe), None)
    }
}
