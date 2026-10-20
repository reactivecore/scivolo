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
        let Type::Or(a, b, _) = *self.types().get(t) else { return Some(t) };
        let (da, db) = (self.dealias(a), self.dealias(b));
        if let (Type::Applied(c1, _), Type::Applied(c2, _)) =
            (self.types().get(da).clone(), self.types().get(db).clone())
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
        match *self.types().get(t) {
            Type::Sym(_, s) => self.symbol(s).kind == Kind::Class,
            Type::Applied(tycon, _) | Type::Refined(tycon, ..) => self.is_class_ref(tycon),
            _ => false,
        }
    }

    fn is_singleton_type(&self, t: TypeId) -> bool {
        match *self.types().get(t) {
            Type::Sym(_, s) => self.symbol(s).kind.is_term(),
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => true,
            _ => false,
        }
    }

    /// `t` widened once unless it is a class type (dotty's `superType`): a singleton to its
    /// type, an abstract type to its upper bound, an alias to its target.
    fn super_type(&mut self, t: TypeId) -> TypeId {
        if self.is_class_ref(t) {
            return t;
        }
        if self.is_singleton_type(t) {
            return self.widen(t);
        }
        let Type::Sym(prefix, s) = *self.types().get(t) else { return t };
        if !matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) {
            return t;
        }
        let info = match prefix {
            Some(p) => self.member_info(p, s),
            None => self.info(s),
        };
        match *self.types().get(info) {
            Type::Bounds(_, hi) => hi,
            Type::Alias(a) => a,
            _ => t,
        }
    }

    /// The base classes the parts of union `t` share, in linearization order (dotty's
    /// `orBaseClasses`).
    fn or_base_classes(&mut self, t: TypeId) -> Option<Vec<SymbolId>> {
        match *self.types().get(t) {
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
                Some(self.linearization(c))
            }
        }
    }

    /// The base type of union `t` at class `c`, the parts' base types merged.
    fn or_base_type(&mut self, t: TypeId, c: SymbolId) -> Option<TypeId> {
        match *self.types().get(t) {
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
            _ => {
                let widened = self.widen(t);
                self.base_type(widened, c)
            }
        }
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
        let c = self.class_symbol(widened);
        c.is_some_and(|c| c == self.builtins().nothing || self.full_name(c) == "scala.Null")
    }

    /// Applications `x` and `y` of the same class merged, their arguments joined by variance
    /// (dotty's `mergeRefinedOrApplied` with `lubArgs`).
    fn merged(&mut self, x: TypeId, y: TypeId) -> Option<TypeId> {
        if x == y {
            return Some(x);
        }
        match (self.types().get(x).clone(), self.types().get(y).clone()) {
            (Type::And(x1, x2), _) => {
                let (m1, m2) = (self.merged(x1, y)?, self.merged(x2, y)?);
                Some(self.glb(m1, m2))
            }
            (Type::Applied(t1, xs), Type::Applied(t2, ys))
                if xs.len() == ys.len() && (t1 == t2 || self.same_static_class(t1, t2).is_some()) =>
            {
                let c = self.class_symbol(t1)?;
                let variances = self.variances(c);
                let mut args = Vec::new();
                for (i, (&p, &q)) in xs.iter().zip(&ys).enumerate() {
                    let interval = |u: &Self, t| match *u.types().get(t) {
                        Type::Bounds(lo, hi) => (lo, hi, true),
                        _ => (t, t, false),
                    };
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
                Some(self.intern_type(Type::Applied(t1, args)))
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

    /// The class of class references `x` and `y` if both are the same top-level class.
    fn same_static_class(&mut self, x: TypeId, y: TypeId) -> Option<SymbolId> {
        let c = self.class_symbol(x)?;
        let static_class = self.symbol(c).owner.is_some_and(|o| self.symbol(o).kind == Kind::Package);
        (static_class && self.class_symbol(y) == Some(c)).then_some(c)
    }
}
