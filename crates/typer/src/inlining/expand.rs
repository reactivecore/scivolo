//! The expansion of one inline call: its arguments bound to the parameters, directly or through
//! proxies, and the body copied with the parameters replaced, retyped and rid of unused bindings
//! (dotty's `Inliner.inlined`, `computeParamBindings`).

use super::copy::TreeTypeMap;
use super::proxies::replace_param;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId, Type, TypeId};

pub struct Expansion<'i, 'a> {
    pub i: &'i mut Inlining<'a>,
    pub call: TNodeId,
    /// The owner of the call: of the proxies and of the expansion's definitions.
    pub owner: SymbolId,
    /// The binding of each parameter: a singleton type, of its argument or of its proxy.
    pub binding: FxHashMap<SymbolId, TypeId>,
    /// The span of the argument of each parameter.
    pub arg_span: FxHashMap<SymbolId, Span>,
    /// The definitions of the proxies, in order.
    pub bindings: Vec<TNodeId>,
}

impl<'i, 'a> Expansion<'i, 'a> {
    pub fn new(i: &'i mut Inlining<'a>, call: TNodeId, owner: SymbolId) -> Self {
        Expansion {
            i,
            call,
            owner,
            binding: FxHashMap::default(),
            arg_span: FxHashMap::default(),
            bindings: Vec::new(),
        }
    }

    pub fn err(&self, what: &'static str) -> Unsupported {
        (self.i.tpd.span(self.call), what)
    }

    /// The bindings and the expansion of the call of the inline method defined by `def`.
    pub fn inlined(mut self, def: TNodeId) -> Result<(Vec<TNodeId>, TNodeId), Unsupported> {
        let (fun, targs, applies) = self.parts();
        let meth = self.i.tpd.sym(fun).ok_or(self.err("inline call"))?;
        self.check_prefix(fun, meth)?;
        let T::DefDef { paramss, rhs: Some(rhs), .. } = *self.i.tpd.tree(def) else {
            return Err(self.err("inline method without a body"));
        };
        let clauses: Vec<Vec<TNodeId>> =
            self.i.tpd.clause_items(paramss).iter().map(|&l| self.i.tpd.items(l).to_vec()).collect();
        let (tclauses, vclauses): (Vec<_>, Vec<_>) = clauses
            .into_iter()
            .partition(|c| c.first().is_some_and(|&p| matches!(self.i.tpd.tree(p), T::TypeDef { .. })));
        let tparams: Vec<TNodeId> = tclauses.into_iter().flatten().collect();
        if tparams.len() != targs.len() || vclauses.len() != applies.len() {
            return Err(self.err("inline call with missing arguments"));
        }
        for (&p, &a) in tparams.iter().zip(&targs) {
            let s = self.i.tpd.sym(p).ok_or(self.err("type parameter"))?;
            self.binding.insert(s, self.i.tpd.tpe(a));
            self.arg_span.insert(s, self.i.tpd.span(a));
        }
        for (params, &apply) in vclauses.iter().zip(&applies) {
            self.bind_clause(params, apply)?;
        }
        let mut m =
            TreeTypeMap { old_owner: Some(meth), new_owner: Some(self.owner), ..TreeTypeMap::default() };
        for (&p, &b) in &self.binding {
            let inline_param = self.i.u.symbol(p).flags.has(INLINE) && self.i.u.symbol(p).kind == Kind::Param;
            let to = if inline_param { self.i.u.info(p) } else { b };
            m.refs.insert(p, to);
        }
        let (binding, arg_span) = (self.binding.clone(), self.arg_span.clone());
        let expansion = self.i.copy(&mut m, rhs, &mut |i, _, t| replace_param(i, &binding, &arg_span, t))?;
        let inline_args: FxHashMap<SymbolId, TNodeId> = self
            .bindings
            .iter()
            .filter_map(|&b| {
                let s = self.i.tpd.sym(b)?;
                let (T::ValDef { rhs: Some(r), .. } | T::DefDef { rhs: Some(r), .. }) = *self.i.tpd.tree(b)
                else {
                    return None;
                };
                self.i.u.symbol(s).flags.has(INLINE).then_some((s, r))
            })
            .collect();
        let expansion = self.i.retype(expansion, &inline_args)?;
        self.i.drop_unused(self.bindings, expansion)
    }

    /// The reference the call applies, its type arguments and its applications, innermost first.
    fn parts(&self) -> (TNodeId, Vec<TNodeId>, Vec<TNodeId>) {
        let (mut t, mut targs, mut applies) = (self.call, Vec::new(), Vec::new());
        loop {
            match *self.i.tpd.tree(t) {
                T::Apply { fun, .. } => {
                    applies.push(t);
                    t = fun;
                }
                T::TypeApply { fun, args } => {
                    targs = self.i.tpd.items(args).to_vec();
                    t = fun;
                }
                _ => break,
            }
        }
        applies.reverse();
        (t, targs, applies)
    }

    /// Checks that the call's prefix needs no proxy: the method is a static object's or a
    /// package's, and an explicit prefix is idempotent.
    fn check_prefix(&mut self, fun: TNodeId, meth: SymbolId) -> Result<(), Unsupported> {
        let owner = self.i.u.symbol(meth).owner.ok_or(self.err("inline method"))?;
        let mut o = Some(owner);
        while let Some(x) = o {
            let s = self.i.u.symbol(x);
            if s.kind != Kind::Package && !self.i.u.is_module_class(x) {
                return Err(self.err("inline method of a class or of a nested object"));
            }
            o = s.owner;
        }
        if let T::Select { qualifier, .. } = *self.i.tpd.tree(fun)
            && !self.i.is_idempotent_expr(qualifier)
        {
            return Err(self.err("inline call of an impure prefix"));
        }
        Ok(())
    }

    /// Binds the parameters `params` to the arguments of application `apply`.
    fn bind_clause(&mut self, params: &[TNodeId], apply: TNodeId) -> Result<(), Unsupported> {
        let T::Apply { fun, args } = *self.i.tpd.tree(apply) else { return Err(self.err("application")) };
        let args = self.i.tpd.items(args).to_vec();
        let fun_tpe = self.i.tpd.tpe(fun);
        let info = self.i.u.widen(fun_tpe);
        let Type::Method { params: formals, .. } = self.i.u.types().get(info).clone() else {
            return Err(self.err("application of a value"));
        };
        if formals.len() != args.len() || params.len() != args.len() {
            return Err(self.err("inline call with missing arguments"));
        }
        for ((&param, &arg), (_, formal)) in params.iter().zip(&args).zip(formals) {
            let p = self.i.tpd.sym(param).ok_or(self.err("parameter"))?;
            self.arg_span.insert(p, self.i.tpd.span(arg));
            let arg = match *self.i.tpd.tree(arg) {
                T::NamedArg { arg: inner, .. } => inner,
                _ => arg,
            };
            let tpe = self.i.tpd.tpe(arg);
            let singleton = match *self.i.u.types().get(tpe) {
                Type::Constant(..) | Type::Str(_) => true,
                Type::Sym(None, s) => !self.i.u.is_type_symbol(s),
                Type::Sym(Some(p), s) => !self.i.u.is_type_symbol(s) && self.i.is_elidable_prefix(p),
                _ => false,
            };
            let bound =
                if singleton && self.i.is_idempotent_path(arg) { tpe } else { self.proxy(p, formal, arg)? };
            self.binding.insert(p, bound);
        }
        Ok(())
    }
}
