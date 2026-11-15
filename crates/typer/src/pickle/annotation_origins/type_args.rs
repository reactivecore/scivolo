//! The origins of an annotated type in an inferred type argument: the arguments it is inferred
//! from and the expected type it is constrained by.

use super::{AnnotationOrigins, Found, Query};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use tasty::{Type, Universe};

impl AnnotationOrigins {
    /// Records, of the inferred type arguments of call `t`, the arguments of its first clause
    /// (those whose parameter has the type argument's type, else all), and of a definition `t`
    /// whose right-hand side is such a call, its written type tree as the expected type the type
    /// arguments are constrained by.
    pub fn add_type_args(&mut self, u: &Universe, tpd: &Tpd, t: TNodeId) {
        let (mut call, pt) = match *tpd.tree(t) {
            T::Apply { fun, args } => {
                let args = tpd.items(args);
                for targ in inferred_type_args(tpd, fun) {
                    let direct = direct_args(u, tpd, fun, targ, args);
                    self.call_args.insert(targ, if direct.is_empty() { args.to_vec() } else { direct });
                }
                return;
            }
            T::ValDef { tpt, rhs: Some(rhs), .. } | T::DefDef { tpt, rhs: Some(rhs), .. } => (rhs, tpt),
            _ => return,
        };
        if matches!(tpd.tree(pt), T::TypeTree { inferred: true }) {
            return;
        }

        while let T::Block { expr, .. } = *tpd.tree(call) {
            call = expr;
        }
        while let T::Apply { fun, .. } = *tpd.tree(call) {
            call = fun;
        }
        for targ in inferred_type_args(tpd, call) {
            self.pts.insert(targ, pt);
        }
    }

    /// The tree inferred type argument `targ` gets `q.annotated` from, of the arguments of its
    /// call's first clause and the expected type, `Some(None)` if they give several, `None` if
    /// `targ` is no inferred type argument.
    pub fn of_type_arg(&self, q: &Query, targ: TNodeId, hops: usize) -> Option<Found> {
        let args = self.call_args.get(&targ).map(Vec::as_slice).unwrap_or_default();
        let pt = self.pts.get(&targ);
        if args.is_empty() && pt.is_none() {
            return None;
        }
        let Some(hops) = hops.checked_sub(1) else { return Some(Some(None)) };
        let from_args = args.iter().map(|&a| self.of_rhs(q, a, hops));
        let from_pt = pt.map(|&pt| self.of_tpt(q, pt, hops));
        Some(from_args.chain(from_pt).fold(None, merge))
    }

    /// The tree the inferred type arguments of type application `call` give, `Some(None)` if
    /// several.
    pub fn of_inferred_type_args(&self, q: &Query, call: TNodeId, hops: usize) -> Found {
        let targs = inferred_type_args(q.tpd, call).into_iter();
        targs.map(|targ| self.of_tpt(q, targ, hops)).fold(None, merge)
    }
}

/// The inferred type argument trees of `fun`, if a type application.
fn inferred_type_args(tpd: &Tpd, fun: TNodeId) -> Vec<TNodeId> {
    let T::TypeApply { args, .. } = *tpd.tree(fun) else { return Vec::new() };
    let args = tpd.items(args).iter().copied();
    args.filter(|&a| matches!(tpd.tree(a), T::TypeTree { inferred: true })).collect()
}

/// The arguments `args` of type application `fun` whose parameter's type is type argument
/// `targ`'s.
fn direct_args(u: &Universe, tpd: &Tpd, fun: TNodeId, targ: TNodeId, args: &[TNodeId]) -> Vec<TNodeId> {
    let Some(Type::Method { params, .. }) = tpd.get(fun).tpe.map(|t| *u.ty(t)) else { return Vec::new() };
    let params = u.params_of(params).iter().map(|&(_, t)| Some(t));
    (params.zip(args)).filter(|&(t, _)| t == tpd.get(targ).tpe).map(|(_, &a)| a).collect()
}

/// The tree both `a` and `b` give, `Some(None)` if they give different ones.
fn merge(a: Found, b: Found) -> Found {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(Some(x)), Some(Some(y))) if x == y => Some(Some(x)),
        _ => Some(None),
    }
}
