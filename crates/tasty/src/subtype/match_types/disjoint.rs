//! Whether two types provably have no value in common (dotty's `provablyDisjoint`).

use super::super::Cmp;
use super::super::refs::Meaning;
use crate::tags::{CASE, ENUM, FINAL, OBJECT, TRAIT};
use crate::types::{Type, TypeId};
use crate::{ANY, Kind, NOTHING, SymbolId};
use rustc_hash::FxHashSet;

/// How many steps `boundary` takes at most.
const MAX_STEPS: u32 = 64;

pub type Pending = FxHashSet<(TypeId, TypeId)>;

impl Cmp<'_> {
    pub fn provably_disjoint(&mut self, a: TypeId, b: TypeId) -> bool {
        self.disjoint_in(a, b, &mut Pending::default())
    }

    pub fn disjoint_in(&mut self, a: TypeId, b: TypeId, pending: &mut Pending) -> bool {
        let (x, y) = (self.boundary(a), self.boundary(b));
        if pending.contains(&(x, y)) {
            return false;
        }
        if x == NOTHING || y == NOTHING || self.is_nothing_class(x) || self.is_nothing_class(y) {
            return true;
        }
        match (self.u.types().get(x).clone(), self.u.types().get(y).clone()) {
            (_, Type::Or(p, q, _)) => {
                return self.disjoint_in(x, p, pending) && self.disjoint_in(x, q, pending);
            }
            (_, Type::And(p, q)) => {
                return self.disjoint_in(x, p, pending) || self.disjoint_in(x, q, pending);
            }
            (Type::Or(p, q, _), _) => {
                return self.disjoint_in(p, y, pending) && self.disjoint_in(q, y, pending);
            }
            (Type::And(p, q), _) => {
                return self.disjoint_in(p, y, pending) || self.disjoint_in(q, y, pending);
            }
            _ => {}
        }
        let any_kind = self.u.builtins().any_kind;
        if [x, y].iter().any(|&t| self.class_of(t) == Some(any_kind)) {
            return false;
        }
        match (self.u.types().get(x).clone(), self.u.types().get(y).clone()) {
            (Type::Lambda { params: p1, result: r1, .. }, Type::Lambda { params: p2, result: r2, .. }) => {
                return p1.len() != p2.len() || self.disjoint_in(r1, r2, pending);
            }
            (Type::Lambda { .. }, _) | (_, Type::Lambda { .. }) => return true,
            _ => {}
        }
        match (self.unique_value(x), self.unique_value(y)) {
            (Some(Some(s1)), Some(Some(s2))) => return s1 != s2,
            (Some(None), Some(None)) => return x != y,
            (Some(_), Some(_)) => return true,
            (Some(_), None) => return !self.value_derives(x, y),
            (None, Some(_)) => return !self.value_derives(y, x),
            (None, None) => {}
        }
        let (Some(c1), Some(c2)) = (self.class_of(x), self.class_of(y)) else { return false };
        self.disjoint_classes(c1, c2) || self.common_base_with_disjoint_args(x, y, (c1, c2), pending)
    }

    /// `t`'s disjointness boundary ⌈t⌉: class types, unique values, unions, intersections and
    /// lambdas as they are, others through their supertype.
    fn boundary(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..MAX_STEPS {
            t = match self.u.types().get(t).clone() {
                Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => {
                    let Some(s) = self.named(t).and_then(|n| n.sym) else { return t };
                    let sym = self.u.symbol(s);
                    let (kind, flags) = (sym.kind, sym.flags);
                    match kind {
                        Kind::Class if s == self.u.builtins().singleton => return ANY,
                        Kind::Class if !self.u.type_params(s).is_empty() => {
                            return self.eta_expand(t).unwrap_or(t);
                        }
                        Kind::Class => return t,
                        Kind::Type | Kind::TypeParam => match self.named(t).map(|n| n.meaning) {
                            Some(Meaning::Alias(a)) => self.normal(a),
                            Some(Meaning::Abstract(_, hi)) => self.normal(hi),
                            _ => return ANY,
                        },
                        _ if flags.has(ENUM) && flags.has(CASE) => return t,
                        _ => match self.u.widen_once(t) {
                            Some(w) => self.normal(w),
                            None => return ANY,
                        },
                    }
                }
                Type::Applied(tycon, args) => match self.named(tycon).and_then(|n| n.sym) {
                    Some(s) if self.u.symbol(s).kind == Kind::Class => return t,
                    _ => match self.fold_compiletime(t) {
                        Some(folded) => return folded,
                        None => match self.applied_super_type(tycon, &args) {
                            Some(sup) => self.normal(sup),
                            None => return ANY,
                        },
                    },
                },
                Type::And(..) | Type::Or(..) | Type::Constant(..) | Type::Str(_) | Type::Lambda { .. } => {
                    return t;
                }
                Type::Var(v) => match self.vars.instance(v) {
                    Some(i) => i,
                    None => self.vars.bounds(v).1,
                },
                Type::This(_) | Type::Skolem(..) | Type::Super(..) => match self.u.widen_once(t) {
                    Some(w) => self.normal(w),
                    None => return ANY,
                },
                Type::Annotated(p, _) | Type::Refined(p, ..) | Type::Rec(_, p) | Type::ByName(p) => p,
                Type::Match { bound, .. } => self.normal(bound),
                Type::Bounds(_, hi) => hi,
                Type::Wildcard(Some(b)) => b,
                _ => return ANY,
            };
        }
        ANY
    }

    fn is_nothing_class(&mut self, t: TypeId) -> bool {
        let nothing = self.u.builtins().nothing;
        matches!(*self.u.types().get(t), Type::Sym(_, s) if s == nothing)
            || matches!(self.u.types().get(t), Type::TypeRef(..)) && self.u.resolve_ref(t) == Some(nothing)
    }

    /// The class of class type `t`.
    fn class_of(&mut self, t: TypeId) -> Option<SymbolId> {
        let tycon = match *self.u.types().get(t) {
            Type::Applied(tycon, _) => tycon,
            _ => t,
        };
        let s = self.named(tycon)?.sym?;
        (self.u.symbol(s).kind == Kind::Class).then_some(s)
    }

    /// For a boundary that is a unique value: `Some(Some(s))` for enum case `s`, `Some(None)`
    /// for a constant.
    fn unique_value(&mut self, t: TypeId) -> Option<Option<SymbolId>> {
        match self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) => Some(None),
            Type::Sym(..) | Type::TermRef(..) => {
                let s = self.named(t)?.sym?;
                self.u.symbol(s).kind.is_term().then_some(Some(s))
            }
            _ => None,
        }
    }

    /// Whether unique value `v`'s class derives from the class of `t`, true if that is unknown.
    fn value_derives(&mut self, v: TypeId, t: TypeId) -> bool {
        let Some(c) = self.class_of(t) else { return true };
        let w = self.u.widen(v);
        match self.u.class_symbol(w) {
            Some(vc) => self.u.derives_from(vc, c),
            None => true,
        }
    }

    /// Whether classes `c1` and `c2` have no common instance (dotty's `provablyDisjointClasses`
    /// without the decomposition of sealed classes).
    fn disjoint_classes(&mut self, c1: SymbolId, c2: SymbolId) -> bool {
        if self.u.derives_from(c1, c2) || self.u.derives_from(c2, c1) {
            return false;
        }
        let is_final =
            |s: &mut Self, c: SymbolId| s.u.symbol(c).flags.has(FINAL) || s.u.symbol(c).flags.has(OBJECT);
        if is_final(self, c1) || is_final(self, c2) {
            return true;
        }
        let (Some(b1), Some(b2)) = (self.smallest_non_trait_base(c1), self.smallest_non_trait_base(c2))
        else {
            return false;
        };
        !(self.u.derives_from(b1, b2) || self.u.derives_from(b2, b1))
    }

    fn smallest_non_trait_base(&mut self, c: SymbolId) -> Option<SymbolId> {
        self.u.linearization(c).into_iter().find(|&b| !self.u.symbol(b).flags.has(TRAIT))
    }
}
