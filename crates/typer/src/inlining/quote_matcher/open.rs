// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! Higher-order term holes: an open scrutinee term matched, then made a closure over the
//! scrutinee's definitions the hole's arguments capture (dotty's `MatchResult.OpenTree`).

use super::{Captured, Hole, Matched, Matcher};
use crate::tpd::{List, TNodeId, Tree as T};
use tasty::{Kind, SymbolId, Type};

impl Matcher<'_> {
    /// `scrutinee =?= $hole[T, targs](args)`: the scrutinee if it refers to no definition the
    /// pattern pairs but those the identifiers `args` and the type parameters `targs` capture.
    pub fn open(&mut self, s: TNodeId, targs: List, args: List) -> Matched {
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
                    if self.u.poly(info).is_some() {
                        return Err("a higher-order hole capturing a polymorphic method");
                    }
                    Some(self.tpd.get(a).tpe.ok_or(shape)?)
                }
                _ => None,
            };
            params.push(Captured { name: self.u.symbol(p).name.clone(), sym: captured, eta });
        }
        let mut tparams = Vec::new();
        if let Some(&targs) = self.tpd.items(targs).get(1) {
            for t in self.unrolled(targs) {
                let p = self.tpd.get(t).tpe.map(|t| self.u.types().get(t).clone());
                let Some(Type::Sym(None, p)) = p else { return Err(shape) };
                let Some(&(captured, _)) = self.env.iter().find(|&&(_, b)| b == p) else { return Err(shape) };
                tparams.push(captured);
            }
        }
        let captured: Vec<SymbolId> = params.iter().map(|c| c.sym).chain(tparams.iter().copied()).collect();
        if self.refers_to_env(s, &captured) {
            return Ok(false);
        }
        self.matched.push(Hole::Open(s, params, tparams));
        Ok(true)
    }

    /// Whether `s` refers to a scrutinee definition the environment pairs, but for `except`, by
    /// an identifier or in a type tree (dotty's `freePatternVars`).
    pub fn refers_to_env(&mut self, s: TNodeId, except: &[SymbolId]) -> bool {
        let (tpd, env) = (&*self.tpd, &self.env);
        let free = |x: SymbolId| !except.contains(&x) && env.iter().any(|&(e, _)| e == x);
        let mut types = Vec::new();
        let found = tpd.fold(false, s, &mut |found, t| match tpd.tree(t) {
            T::Ident { .. } => found || tpd.sym(t).is_some_and(free),
            T::TypeTree { .. } => {
                types.extend(tpd.get(t).tpe);
                found
            }
            _ => found,
        });
        let mut mentioned = false;
        for t in types {
            self.u.map_type(t, &mut |u, part| {
                mentioned |= matches!(*u.types().get(part), Type::Sym(None, x) if free(x));
                None
            });
        }
        found || mentioned
    }

    /// The type arguments of a type tree of nested pairs `T1 *: ... *: EmptyTuple` or tuple
    /// `(T1, ..., Tn)` (dotty's `unrollHkNestedPairsTypeTree`).
    fn unrolled(&mut self, t: TNodeId) -> Vec<TNodeId> {
        let T::AppliedTypeTree { tpt, args } = *self.tpd.tree(t) else { return Vec::new() };
        let args = self.tpd.items(args).to_vec();
        let class = self.tpd.get(tpt).tpe.and_then(|c| self.u.class_symbol(c));
        let tuple = class.is_some_and(|c| {
            let name = self.u.full_name(c);
            name.strip_prefix("scala.Tuple")
                .is_some_and(|n| n.parse::<u8>().is_ok_and(|n| (1..=22).contains(&n)))
        });
        match args[..] {
            _ if tuple => args,
            [head, tail] => [vec![head], self.unrolled(tail)].concat(),
            _ => Vec::new(),
        }
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
