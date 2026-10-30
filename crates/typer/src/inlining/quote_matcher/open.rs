// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! Higher-order term holes: an open scrutinee term matched, then made a closure over the
//! scrutinee's definitions the hole's arguments capture (dotty's `MatchResult.OpenTree`).

use super::super::copy::TreeTypeMap;
use super::super::quoted::reown;
use super::super::reflect_beta::NoHost;
use super::super::{Inlining, Reflect};
use super::{Captured, Hole, Matched, Matcher};
use crate::tpd::{List, TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::names::FreshNames;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, Symbol, SymbolId, Type};

impl Matcher<'_> {
    /// `scrutinee =?= $hole(args)`: the scrutinee if it refers to no definition the pattern
    /// pairs but those the identifiers `args` capture.
    pub fn open(&mut self, s: TNodeId, args: List) -> Matched {
        let shape = "a higher-order hole capturing other than identifiers";
        let [arg] = self.tpd.items(args)[..] else { return Err(shape) };
        let seq = match *self.tpd.tree(arg) {
            T::Typed { expr, .. } => expr,
            _ => arg,
        };
        let T::SeqLiteral { elems, .. } = *self.tpd.tree(seq) else { return Err(shape) };
        let mut params = Vec::new();
        for &a in self.tpd.items(elems) {
            let id = self.captured_ident(a);
            let (T::Ident { .. }, Some(p)) = (self.tpd.tree(id), self.tpd.sym(id)) else { return Err(shape) };
            let Some(&(captured, _)) = self.env.iter().find(|&&(_, b)| b == p) else { return Err(shape) };
            let eta = match self.u.symbol(captured).kind {
                Kind::Def => {
                    let info = self.u.info(captured);
                    let types = self.u.types();
                    let curried = match types.get(info) {
                        Type::Poly { .. } => true,
                        Type::Method { result, .. } => matches!(types.get(*result), Type::Method { .. }),
                        _ => false,
                    };
                    if curried {
                        return Err("a higher-order hole capturing a polymorphic or curried method");
                    }
                    Some(self.tpd.get(a).tpe.ok_or(shape)?)
                }
                _ => None,
            };
            params.push(Captured { name: self.u.symbol(p).name.clone(), sym: captured, eta });
        }
        let captured: Vec<SymbolId> = params.iter().map(|c| c.sym).collect();
        if self.refers_to_env(s, &captured) {
            return Ok(false);
        }
        self.matched.push(Hole::Open(s, params));
        Ok(true)
    }

    /// The identifier a higher-order hole's argument `t` captures, through applications,
    /// ascriptions and eta expansions (dotty's `getCapturedIdent`).
    fn captured_ident(&self, t: TNodeId) -> TNodeId {
        match *self.tpd.tree(t) {
            T::Apply { fun: inner, .. } | T::Typed { expr: inner, .. } => self.captured_ident(inner),
            T::Block { stats, expr } if matches!(self.tpd.tree(expr), T::Closure { .. }) => {
                match self.tpd.items(stats).first().map(|&d| self.tpd.tree(d)) {
                    Some(&T::DefDef { rhs: Some(rhs), .. }) => self.captured_ident(rhs),
                    _ => t,
                }
            }
            _ => t,
        }
    }
}

impl Reflect<'_> {
    /// The closure `(params) => t` owned by `owner`, its parameters, named as `params` name them,
    /// standing for the scrutinee's symbols there.
    pub fn open_closure(
        &mut self,
        owner: SymbolId,
        t: TNodeId,
        params: &[Captured],
    ) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(t);
        let symbol = |name: &str, kind, owner| Symbol {
            name: name.to_owned(),
            kind,
            owner: Some(owner),
            flags: Flags::default().with(SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        };
        let anonfun = self.u.define(symbol("$anonfun", Kind::Def, owner));
        let (mut syms, mut types, mut param_trees) = (FxHashMap::default(), Vec::new(), Vec::new());
        let mut methods = Vec::new();
        for c in params {
            let p = self.u.define(symbol(&c.name, Kind::Param, anonfun));
            let tpe = match c.eta {
                Some(function) => {
                    methods.push(p);
                    function
                }
                None => {
                    let info = self.u.info(c.sym);
                    self.u.widen(info)
                }
            };
            self.u.set_info(p, tpe);
            syms.insert(c.sym, p);
            types.push(tpe);
            let tpt = self.type_tree(tpe);
            param_trees.push(self.build()?.val_def(p, tpt, None, span));
        }
        let result = self.tpd.tpe(t);
        let result = self.u.widen(result);
        let binder = self.u.fresh_binder();
        let method_params = params.iter().map(|c| c.name.clone()).zip(types.iter().copied()).collect();
        let info = Type::Method { binder, params: method_params, result, implicit: false, contextual: false };
        let info = self.u.intern_type(info);
        self.u.set_info(anonfun, info);
        let body = self.copied(t, syms)?;
        reown(self.tpd, self.u, body, anonfun);
        self.applies_through(body, &methods)?;
        let tpt = self.type_tree(result);
        let def = self.build()?.def_def(anonfun, &[&param_trees], tpt, Some(body), span);
        let meth_tpe = self.u.intern_type(Type::Sym(None, anonfun));
        let meth = self.build()?.ident(anonfun, meth_tpe, span);
        let class = self.u.class_ref(&format!("scala.Function{}", params.len()));
        types.push(result);
        let fun_tpe = self.u.intern_type(Type::Applied(class, types));
        let env = self.tpd.list(&[]);
        let closure = self.tpd.node(T::Closure { env, meth, tpt: None }, span, Some(fun_tpe), None);
        let stats = self.tpd.list(&[def]);
        Ok(self.build()?.block(stats, closure, span))
    }

    /// A copy of `t` with the references to the symbols of `syms` replaced by references to
    /// those they map to.
    fn copied(&mut self, t: TNodeId, syms: FxHashMap<SymbolId, SymbolId>) -> Result<TNodeId, &'static str> {
        let mut fresh = FreshNames::default();
        let mut host = NoHost;
        let mut i =
            Inlining::new((&mut *self.u, &mut *self.tpd), (&mut *self.names, &mut fresh), &mut host, false);
        let mut m = TreeTypeMap { syms, ..TreeTypeMap::default() };
        i.copy(&mut m, t, &mut |_, _, _| None).map_err(|(_, what)| what)
    }

    /// Rewrites the calls `f(args)` in `t` of the parameters `functions`, which stand for
    /// methods, into `f.apply(args)`.
    fn applies_through(&mut self, t: TNodeId, functions: &[SymbolId]) -> Result<(), &'static str> {
        let mut todo = vec![t];
        while let Some(n) = todo.pop() {
            todo.extend(self.tpd.children(n));
            let T::Apply { fun, args } = *self.tpd.tree(n) else { continue };
            let Some(f) = self.tpd.sym(fun).filter(|f| functions.contains(f)) else { continue };
            let tpe = self.u.info(f);
            let apply =
                self.u.member_of_type(tpe, "apply").into_iter().find(|&m| self.u.symbol(m).kind.is_term());
            let apply = apply.ok_or("a captured method without a function type")?;
            let span = self.tpd.span(fun);
            let select = self.build()?.select(fun, apply, span);
            self.tpd.get_mut(n).tree = T::Apply { fun: select, args };
        }
        Ok(())
    }
}
