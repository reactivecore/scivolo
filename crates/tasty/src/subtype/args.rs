// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Arguments of applied types compared at their places (dotty's `isSubArgs`).

use super::Cmp;
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING, SymbolId};

impl Cmp<'_> {
    /// Whether each of `args1` conforms to the argument of `args2` at its place, by the
    /// variance of the parameter there; a wildcard of `args1` narrowed by the bounds of its
    /// parameter among `tparams`.
    pub fn sub_args(
        &mut self,
        args1: &[TypeId],
        args2: &[TypeId],
        variances: &[i8],
        tparams: &[SymbolId],
    ) -> bool {
        let tparams = if tparams.len() == args2.len() { tparams } else { &[] };
        args1.len() == args2.len()
            && (0..args1.len()).all(|i| {
                let variance = variances.get(i).copied().unwrap_or(0);
                let wildcards =
                    self.written_bounds(args1[i]).is_some() && self.written_bounds(args2[i]).is_some();
                let bounds = match tparams.get(i) {
                    Some(&p) if wildcards => self.param_bounds(p, tparams, args2),
                    _ => None,
                };
                self.sub_arg(args1[i], args2[i], variance, bounds)
            })
    }

    fn sub_arg(&mut self, x: TypeId, y: TypeId, variance: i8, bounds: Option<(TypeId, TypeId)>) -> bool {
        match (self.written_bounds(x), self.written_bounds(y)) {
            (Some(b1), Some((lo2, hi2))) => {
                let (lo1, hi1) = bounds.map_or(b1, |b| self.and_bounds(b1, b));
                self.recur(lo2, lo1) && self.recur(hi1, hi2)
            }
            (None, Some((lo2, hi2))) => self.recur(lo2, x) && self.recur(x, hi2),
            (Some((lo1, hi1)), None) => match variance {
                1 => self.recur(hi1, y),
                -1 => self.recur(y, lo1),
                _ => false,
            },
            (None, None) => match variance {
                1 => self.recur(x, y),
                -1 => self.recur(y, x),
                _ => self.recur(y, x) && self.recur(x, y),
            },
        }
    }

    /// The bounds of type parameter `p` with the parameters `tparams` replaced by `args`
    /// (dotty's `paramBounds`), `None` if they are empty or refer to a wildcard argument.
    fn param_bounds(
        &mut self,
        p: SymbolId,
        tparams: &[SymbolId],
        args: &[TypeId],
    ) -> Option<(TypeId, TypeId)> {
        let info = self.u.info(p);
        let (lo, hi) = self.u.bounds(info)?;
        if lo == NOTHING && hi == ANY {
            return None;
        }
        let class = self.u.symbol(p).owner;
        let mut wild = false;
        let mut subst = |t: TypeId, u: &mut crate::Universe| {
            u.map_type(t, &mut |u, x| match u.class_param(x) {
                Some((owner, i)) if Some(owner) == class && i < tparams.len() => {
                    let arg = args[i];
                    wild |= matches!(u.ty(arg), Type::Bounds(..) | Type::Wildcard(_));
                    Some(arg)
                }
                _ => None,
            })
        };
        let bounds = (subst(lo, self.u), subst(hi, self.u));
        (!wild).then_some(bounds)
    }

    /// The intersection of bounds `a` and `b` (dotty's `TypeBounds.&`).
    fn and_bounds(&mut self, (lo1, hi1): (TypeId, TypeId), (lo2, hi2): (TypeId, TypeId)) -> (TypeId, TypeId) {
        if self.recur_frozen(lo1, lo2) && self.recur_frozen(hi2, hi1) {
            return (lo2, hi2);
        }
        if self.recur_frozen(lo2, lo1) && self.recur_frozen(hi1, hi2) {
            return (lo1, hi1);
        }
        (self.lub(lo1, lo2), self.glb(hi1, hi2))
    }

    /// The bounds of a `?` argument, annotated or not; a `Wildcard` compares as a type.
    fn written_bounds(&mut self, t: TypeId) -> Option<(TypeId, TypeId)> {
        match *self.u.ty(t) {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            Type::Annotated(b, _) => self.written_bounds(b),
            _ => None,
        }
    }

    /// The bounds of a wildcard argument, annotated or not.
    pub fn arg_bounds(&mut self, t: TypeId) -> Option<(TypeId, TypeId)> {
        match *self.u.ty(t) {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            Type::Wildcard(None) => Some((NOTHING, ANY)),
            Type::Wildcard(Some(b)) | Type::Annotated(b, _) => self.arg_bounds(b),
            _ => None,
        }
    }
}
