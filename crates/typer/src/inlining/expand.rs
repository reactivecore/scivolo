//! The expansion of one inline call: its arguments bound to the parameters, directly or through
//! proxies, and the body copied with the parameters replaced, retyped and rid of unused bindings
//! (dotty's `Inliner.inlined`, `computeParamBindings`).

use super::copy::TreeTypeMap;
use super::proxies::replace_param;
use super::skolems::call_skolems;
use super::this_proxy::replace_this;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::deskolemize::has_skolem;
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
        let this = self.bind_this(fun, meth, rhs)?;
        let this_tpe = this.map(|(cls, proxy)| (cls, self.i.u.intern_type(Type::Sym(None, proxy))));
        let mut m = TreeTypeMap {
            old_owner: Some(meth),
            new_owner: Some(self.owner),
            this: this_tpe,
            ..TreeTypeMap::default()
        };
        for (&p, &b) in &self.binding {
            let inline_param = self.i.u.symbol(p).flags.has(INLINE) && self.i.u.symbol(p).kind == Kind::Param;
            let to = if inline_param { self.i.u.info(p) } else { b };
            m.refs.insert(p, to);
        }
        self.i.type_this_qualifiers(rhs);
        let (binding, arg_span) = (self.binding.clone(), self.arg_span.clone());
        let call_span = self.i.tpd.span(self.call);
        let expansion = self.i.copy(&mut m, rhs, &mut |i, _, t| {
            replace_param(i, &binding, &arg_span, t)
                .or_else(|| this.and_then(|(cls, proxy)| replace_this(i, cls, proxy, call_span, t)))
        })?;
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
        let expansion = self.i.retype(expansion, &inline_args, self.owner)?;
        self.i.drop_unused(self.bindings, expansion, self.owner)
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

    /// Binds the parameters `params` to the arguments of application `apply`.
    fn bind_clause(&mut self, params: &[TNodeId], apply: TNodeId) -> Result<(), Unsupported> {
        let T::Apply { fun, args } = *self.i.tpd.tree(apply) else { return Err(self.err("application")) };
        let args = self.i.tpd.items(args).to_vec();
        let fun_tpe = self.i.tpd.tpe(fun);
        let info = self.i.u.widen(fun_tpe);
        let Type::Method { binder, params: formals, result, .. } = self.i.u.types().get(info).clone() else {
            return Err(self.err("application of a value"));
        };
        if formals.len() != args.len() || params.len() != args.len() {
            return Err(self.err("inline call with missing arguments"));
        }
        let spans: Vec<Span> = args.iter().map(|&a| self.i.tpd.span(a)).collect();
        let args: Vec<TNodeId> = args
            .into_iter()
            .map(|a| match *self.i.tpd.tree(a) {
                T::NamedArg { arg, .. } => arg,
                _ => a,
            })
            .collect();
        let syms = params
            .iter()
            .map(|&p| self.i.tpd.sym(p).ok_or(self.err("parameter")))
            .collect::<Result<Vec<_>, _>>()?;
        let tpes: Vec<TypeId> = args.iter().map(|&a| self.i.tpd.tpe(a)).collect();
        let applied = self.i.tpd.tpe(apply);
        let skolems = match call_skolems(self.i.u, (result, applied), (binder, &syms), &tpes) {
            Some(s) => s,
            None if has_skolem(self.i.u, applied) => return Err(self.err("argument without its skolem")),
            None => vec![None; args.len()],
        };
        for ((((&p, &arg), (_, formal)), skolem), span) in
            syms.iter().zip(&args).zip(formals).zip(skolems).zip(spans)
        {
            self.arg_span.insert(p, span);
            let tpe = self.i.tpd.tpe(arg);
            let bound = if self.i.is_singleton_path(tpe) && self.i.is_idempotent_path(arg) {
                tpe
            } else {
                self.proxy(p, formal, arg, skolem)?
            };
            self.binding.insert(p, bound);
        }
        Ok(())
    }
}
