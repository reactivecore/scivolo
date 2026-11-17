//! Rebuilding a type with parts replaced.

use super::{ParamList, Type, TypeId};
use crate::Universe;

/// The full-name prefix of capture checking's annotation classes (`retains`, `retainsByName`,
/// `retainsCap`).
const RETAINS: &str = "scala.annotation.retains";

impl Universe {
    /// Determines whether annotation type `annot` is a capture set.
    pub fn is_capture_set(&mut self, annot: TypeId) -> bool {
        self.class_symbol(annot).is_some_and(|c| self.full_name_starts_with(c, RETAINS))
    }

    /// `t` mapped as `map_type` maps it, and the references of its capture sets too (dotty's
    /// `RetainingAnnotation.mapWith`).
    pub fn map_with_capture_sets(
        &mut self,
        t: TypeId,
        f: &mut dyn FnMut(&mut Self, TypeId) -> Option<TypeId>,
    ) -> TypeId {
        self.map_type(t, &mut |u, x| {
            if let Some(r) = f(u, x) {
                return Some(r);
            }
            let (inner, annot) = u.annotated(x)?;
            if !u.is_capture_set(annot) {
                return None;
            }
            let mapped = u.map_with_capture_sets(inner, f);
            let set = u.map_with_capture_sets(annot, f);
            Some(match (mapped, set) == (inner, annot) {
                true => x,
                false => u.intern_type(Type::Annotated(mapped, set)),
            })
        })
    }

    /// `t` mapped as `map_type` maps it, in a capture-checked file as `map_with_capture_sets`
    /// (dotty's `TypeMap` of an `AnnotatedType`, which maps a capture set where capture checking
    /// is enabled).
    pub fn map_type_and_kept_sets(
        &mut self,
        t: TypeId,
        f: &mut dyn FnMut(&mut Self, TypeId) -> Option<TypeId>,
    ) -> TypeId {
        match self.keeps_captures {
            true => self.map_with_capture_sets(t, f),
            false => self.map_type(t, f),
        }
    }

    /// `t` with every part for which `f` gives a replacement replaced, outermost parts first;
    /// the other parts are rebuilt from their mapped children, an application whose constructor
    /// became a type lambda beta-reduced, one whose constructor became `Nothing` `Nothing`, a union
    /// or intersection of one type twice that type mapped.
    pub fn map_type(&mut self, t: TypeId, f: &mut dyn FnMut(&mut Self, TypeId) -> Option<TypeId>) -> TypeId {
        if let Some(r) = f(self, t) {
            return r;
        }
        let mut m = |u: &mut Self, x: TypeId| u.map_type(x, f);
        let params = |u: &mut Self, ps: ParamList, m: &mut dyn FnMut(&mut Self, TypeId) -> TypeId| {
            let mapped: Vec<_> = (0..u.params_of(ps).len())
                .map(|i| {
                    let (n, p) = u.params_of(ps)[i];
                    (n, m(u, p))
                })
                .collect();
            if mapped[..] == *u.params_of(ps) { ps } else { u.intern_params(&mapped) }
        };
        let mapped = match *self.ty(t) {
            Type::TypeRef(p, n) => Type::TypeRef(m(self, p), n),
            Type::TermRef(p, n) => Type::TermRef(m(self, p), n),
            Type::Sym(Some(p), s) => Type::Sym(Some(m(self, p)), s),
            Type::This(c) => Type::This(m(self, c)),
            Type::Super(t, p) => {
                let t = m(self, t);
                Type::Super(t, m(self, p))
            }
            Type::Applied(c, list) => {
                let mapped = m(self, c);
                if mapped != c && mapped == crate::NOTHING {
                    return mapped;
                }
                let args: Vec<_> = (0..self.types_of(list).len())
                    .map(|i| {
                        let a = self.types_of(list)[i];
                        m(self, a)
                    })
                    .collect();
                if mapped == c && args[..] == *self.types_of(list) {
                    return t;
                }
                let applied = self.intern_applied(mapped, &args);
                let lambda = self.lambda(mapped).is_some();
                return if mapped != c && lambda { self.beta_reduced(applied) } else { applied };
            }
            Type::Alias(a) => Type::Alias(m(self, a)),
            Type::Bounds(lo, hi) => Type::Bounds(m(self, lo), m(self, hi)),
            Type::And(a, b) => match (m(self, a), m(self, b)) {
                (x, y) if (x, y) != (a, b) && x == y => return x,
                (x, y) => Type::And(x, y),
            },
            Type::Or(a, b, soft) => {
                let (x, y) = (m(self, a), m(self, b));
                if a == b && x != a {
                    return x;
                }
                Type::Or(x, y, soft)
            }
            Type::Annotated(a, annot) => {
                let mapped = m(self, a);
                if mapped == a {
                    return t;
                }
                let annotated = self.intern_type(Type::Annotated(mapped, annot));
                return self.carry_annotation_tree(t, annotated);
            }
            Type::Flexible(hi) => {
                let hi = m(self, hi);
                return self.flexible(hi);
            }
            Type::ByName(a) => Type::ByName(m(self, a)),
            Type::Refined(p, n, i) => Type::Refined(m(self, p), n, m(self, i)),
            Type::Rec(b, a) => Type::Rec(b, m(self, a)),
            Type::Match { bound, scrutinee, cases } => {
                let (bound, scrutinee) = (m(self, bound), m(self, scrutinee));
                let cases: Vec<_> = (0..self.types_of(cases).len())
                    .map(|i| {
                        let c = self.types_of(cases)[i];
                        m(self, c)
                    })
                    .collect();
                return self.intern_match(bound, scrutinee, &cases);
            }
            Type::MatchCase(p, b) => Type::MatchCase(m(self, p), m(self, b)),
            Type::Wildcard(Some(b)) => Type::Wildcard(Some(m(self, b))),
            Type::Skolem(_, info) => {
                let mapped = m(self, info);
                return if mapped == info { t } else { self.fresh_skolem(mapped) };
            }
            Type::Method { binder, params: ps, result, implicit, contextual } => {
                let params = params(self, ps, &mut m);
                Type::Method { binder, params, result: m(self, result), implicit, contextual }
            }
            Type::Poly { binder, params: ps, result } => {
                let params = params(self, ps, &mut m);
                Type::Poly { binder, params, result: m(self, result) }
            }
            Type::Lambda { binder, params: ps, result } => {
                let params = params(self, ps, &mut m);
                Type::Lambda { binder, params, result: m(self, result) }
            }
            _ => return t,
        };
        if mapped == *self.ty(t) {
            return t;
        }
        self.intern_type(mapped)
    }
}
