// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The join of a union (dotty's `orDominator`): the intersection of the dominating common base
//! classes of its parts, each applied to the parts' arguments merged.

use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

impl Universe {
    /// The join of union `t`, `t` itself if it is no union; `None` where the parts do not merge.
    pub fn join(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Or(a, b, _) = *self.ty(t) else { return Some(t) };
        let (da, db) = (self.dealias(a), self.dealias(b));
        if let (Type::Applied(c1, _), Type::Applied(c2, _)) = (*self.ty(da), *self.ty(db))
            && c1 == c2
        {
            return self.merged(da, db);
        }
        let (aw, bw) = (self.super_type(a), self.super_type(b));
        if aw != a || bw != b {
            let (sa, sb) = (self.is_singleton_type(a), self.is_singleton_type(b));
            let or = |u: &mut Self, x, y| u.lub(x, y);
            let next = if bw == b {
                or(self, aw, b)
            } else if aw == a {
                or(self, a, bw)
            } else if self.is_subtype(aw, bw) {
                or(self, aw, b)
            } else if self.is_subtype(bw, aw) {
                or(self, a, bw)
            } else if sa && !sb {
                or(self, aw, b)
            } else if sb && !sa {
                or(self, a, bw)
            } else if self.is_subtype(a, bw) {
                return Some(bw);
            } else if self.is_subtype(b, aw) {
                return Some(aw);
            } else {
                or(self, aw, b)
            };
            return self.join(next);
        }
        let common = self.or_base_classes(t)?;
        let mut doms: Vec<SymbolId> = Vec::new();
        for (i, &c) in common.iter().enumerate() {
            if !doms.iter().any(|&d| self.derives_from(d, c)) {
                doms.push(c);
            }
            if common[i..] == self.linearization(c)[..] {
                break;
            }
        }
        let mut result: Option<TypeId> = None;
        for &c in doms.iter().rev() {
            let base = self.or_base_type(t, c)?;
            result = Some(match result {
                Some(r) => self.intern_type(Type::And(r, base)),
                None => base,
            });
        }
        result
    }

    /// Whether `t` is a class type, possibly applied.
    fn is_class_ref(&self, t: TypeId) -> bool {
        match *self.ty(t) {
            Type::Sym(_, s) => self.symbol(s).kind == Kind::Class,
            Type::Applied(tycon, _) | Type::Refined(tycon, ..) => self.is_class_ref(tycon),
            _ => false,
        }
    }

    fn is_singleton_type(&self, t: TypeId) -> bool {
        match *self.ty(t) {
            Type::Sym(_, s) => self.symbol(s).kind.is_term(),
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => true,
            _ => false,
        }
    }

    /// `t` widened once unless it is a class type (dotty's `superType`): a singleton to its
    /// type, an abstract type to its upper bound, an alias to its target, an application of a
    /// match type alias to the match type, a match type to its bound.
    fn super_type(&mut self, t: TypeId) -> TypeId {
        if self.is_class_ref(t) {
            return t;
        }
        if self.is_singleton_type(t) {
            return self.widen(t);
        }
        if let Some(hi) = self.opaque_upper(t) {
            return hi;
        }
        match *self.ty(t) {
            Type::Match { bound, .. } => return bound,
            Type::Applied(..) => {
                let dealiased = self.dealias(t);
                if matches!(self.ty(dealiased), Type::Match { .. }) {
                    return dealiased;
                }
            }
            _ => {}
        }
        let Type::Sym(prefix, s) = *self.ty(t) else { return t };
        if !matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) {
            return t;
        }
        let arg =
            prefix.filter(|_| self.symbol(s).kind == Kind::TypeParam).and_then(|p| self.arg_for_param(p, s));
        let info = match (arg, prefix) {
            (Some(arg), _) => arg,
            (None, Some(p)) => self.member_info(p, s),
            (None, None) => self.info(s),
        };
        match *self.ty(info) {
            Type::Bounds(_, hi) => hi,
            Type::Alias(a) => a,
            _ => arg.unwrap_or(t),
        }
    }

    /// The base classes the parts of union `t` share, in linearization order (dotty's
    /// `orBaseClasses`).
    fn or_base_classes(&mut self, t: TypeId) -> Option<Vec<SymbolId>> {
        match *self.ty(t) {
            Type::Or(a, b, _) => {
                if let Some(other) = self.non_bottom_part(a, b) {
                    return self.or_base_classes(other);
                }
                let (xs, ys) = (self.or_base_classes(a)?, self.or_base_classes(b)?);
                Some(xs.into_iter().filter(|x| ys.contains(x)).collect())
            }
            Type::And(a, b) => {
                let (xs, ys) = (self.or_base_classes(a)?, self.or_base_classes(b)?);
                let mut merged = Vec::new();
                for y in ys {
                    if !xs.contains(&y) {
                        merged.push(y);
                    } else if !self.is_trait(y) {
                        break;
                    }
                }
                merged.extend(xs);
                Some(merged)
            }
            _ => {
                let widened = self.widen(t);
                let c = self.class_symbol(widened)?;
                let b = self.defn();
                if c == b.null && self.null_mode().explicit {
                    return Some(vec![b.null, b.matchable, b.any]);
                }
                Some(self.linearization(c))
            }
        }
    }

    /// The base type of union `t` at class `c`, the parts' base types merged.
    fn or_base_type(&mut self, t: TypeId, c: SymbolId) -> Option<TypeId> {
        match *self.ty(t) {
            Type::Or(a, b, _) => {
                if let Some(other) = self.non_bottom_part(a, b) {
                    return self.or_base_type(other, c);
                }
                let (x, y) = (self.or_base_type(a, c)?, self.or_base_type(b, c)?);
                self.merged(x, y)
            }
            Type::And(a, b) => match (self.or_base_type(a, c), self.or_base_type(b, c)) {
                (Some(x), Some(y)) => Some(self.glb(x, y)),
                (x, y) => x.or(y),
            },
            _ if self.is_static_symbol(c) && self.type_params(c).is_empty() => {
                let widened = self.widen(t);
                self.base_type(widened, c).map(|_| self.static_class_ref(c))
            }
            _ => {
                let widened = self.widen(t);
                self.base_type(widened, c)
            }
        }
    }

    /// The reference to static class `c` through the `this` of its enclosing objects.
    fn static_class_ref(&mut self, c: SymbolId) -> TypeId {
        let prefix = self.symbol(c).owner.filter(|&o| self.symbol(o).kind == Kind::Class).map(|o| {
            let owner = self.static_class_ref(o);
            self.intern_type(Type::This(owner))
        });
        self.intern_type(Type::Sym(prefix, c))
    }

    /// The other part of union `a | b` if one part is a bottom type, `Nothing` or `Null`,
    /// conforming to it.
    fn non_bottom_part(&mut self, a: TypeId, b: TypeId) -> Option<TypeId> {
        if self.is_bottom(a) && self.is_subtype(a, b) {
            return Some(b);
        }
        (self.is_bottom(b) && self.is_subtype(b, a)).then_some(a)
    }

    /// Whether `t` is of class `Nothing` or `Null` (dotty's `isBottomTypeAfterErasure`).
    fn is_bottom(&mut self, t: TypeId) -> bool {
        let widened = self.widen(t);
        let (c, d) = (self.class_symbol(widened), self.defn());
        c.is_some_and(|c| c == d.nothing || c == d.null)
    }

    /// Applications `x` and `y` of the same class merged, their arguments joined by variance
    /// (dotty's `mergeRefinedOrApplied` with `lubArgs`).
    fn merged(&mut self, x: TypeId, y: TypeId) -> Option<TypeId> {
        if x == y {
            return Some(x);
        }
        match (*self.ty(x), *self.ty(y)) {
            (Type::And(x1, x2), _) => {
                let (m1, m2) = (self.merged(x1, y)?, self.merged(x2, y)?);
                Some(self.glb(m1, m2))
            }
            (Type::Applied(t1, xs), Type::Applied(t2, ys))
                if self.types_of(xs).len() == self.types_of(ys).len()
                    && (t1 == t2 || self.same_static_class(t1, t2).is_some()) =>
            {
                self.class_symbol(t1)?;
                let (xs, ys) = (self.types_of(xs).to_vec(), self.types_of(ys).to_vec());
                let variances = self.constructor_variances(t1);
                let mut args = Vec::new();
                for (i, (&p, &q)) in xs.iter().zip(&ys).enumerate() {
                    let interval = |u: &Self, t| u.bounds(t).map_or((t, t, false), |(lo, hi)| (lo, hi, true));
                    let ((plo, phi, pb), (qlo, qhi, qb)) = (interval(self, p), interval(self, q));
                    let same = p == q || !pb && !qb && self.is_subtype(p, q) && self.is_subtype(q, p);
                    args.push(match variances.get(i).copied().unwrap_or(0) {
                        _ if same => p,
                        1 => self.lub(phi, qhi),
                        -1 => self.glb(plo, qlo),
                        _ => {
                            let (lo, hi) = (self.glb(plo, qlo), self.lub(phi, qhi));
                            self.intern_type(Type::Bounds(lo, hi))
                        }
                    });
                }
                Some(self.intern_applied(t1, &args))
            }
            (Type::Sym(p1, s1), Type::Sym(p2, s2)) if s1 == s2 && p1 == p2 => Some(x),
            (Type::Sym(..) | Type::TypeRef(..), Type::Sym(..) | Type::TypeRef(..)) => {
                let c = self.same_static_class(x, y)?;
                self.type_params(c).is_empty().then_some(x)
            }
            (_, Type::And(y1, y2)) => {
                let (m1, m2) = (self.merged(x, y1)?, self.merged(x, y2)?);
                Some(self.glb(m1, m2))
            }
            _ => None,
        }
    }

    /// The class of class references `x` and `y` if both are the same static class.
    fn same_static_class(&mut self, x: TypeId, y: TypeId) -> Option<SymbolId> {
        let c = self.class_symbol(x)?;
        (self.is_static_symbol(c) && self.class_symbol(y) == Some(c)).then_some(c)
    }
}
