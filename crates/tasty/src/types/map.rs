//! Rebuilding a type with parts replaced.

use super::{ParamList, Type, TypeId};
use crate::tags::PRIVATE;
use crate::{Kind, SymbolId, Universe};

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

    /// Determines the value a reference to `s` means on its new prefix `q` (dotty's
    /// `NamedType.withPrefix`): the member of that name of `q`, as the `val a` of an object
    /// overriding `a` of its parent; a private value or one without such a member stays `s`.
    pub fn val_on_prefix(&mut self, q: TypeId, s: SymbolId) -> SymbolId {
        if self.symbol(s).kind != Kind::Val || self.symbol(s).flags.has(PRIVATE) {
            return s;
        }
        let name = self.interned_name(s);
        self.resolve(q, name, false).filter(|&m| self.symbol(m).kind == Kind::Val).unwrap_or(s)
    }

    /// Determines whether `t` is `Nothing` or an alias of it (dotty's `isRef(defn.NothingClass)`),
    /// following declared alias right-hand sides without seeing them from a prefix.
    pub fn is_nothing_ref(&mut self, mut t: TypeId) -> bool {
        let nothing = self.defn().nothing;
        for _ in 0..super::MAX_STEPS {
            if let Type::Annotated(u, _) = *self.ty(t) {
                t = u;
                continue;
            }
            let Some(s) = self.sym_of(t) else { return false };
            if s == nothing {
                return true;
            }
            let Some(a) = self.alias_of(s) else { return false };
            t = a;
        }
        false
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
            let mut changed: Option<Vec<_>> = None;
            for i in 0..u.params_of(ps).len() {
                let (n, p) = u.params_of(ps)[i];
                let x = m(u, p);
                record(&mut changed, u.params_of(ps), i, (n, x));
            }
            changed.map_or(ps, |mapped| u.intern_params(&mapped))
        };
        let mapped = match *self.ty(t) {
            Type::TypeRef(p, n) => Type::TypeRef(m(self, p), n),
            Type::TermRef(p, n) => Type::TermRef(m(self, p), n),
            Type::Sym(Some(p), s) => {
                let q = m(self, p);
                Type::Sym(Some(q), if q == p { s } else { self.val_on_prefix(q, s) })
            }
            Type::This(c) => Type::This(m(self, c)),
            Type::Super(t, p) => {
                let t = m(self, t);
                Type::Super(t, m(self, p))
            }
            Type::Applied(c, list) => {
                let mapped = m(self, c);
                if mapped != c && self.is_nothing_ref(mapped) {
                    return crate::NOTHING;
                }
                let mut changed: Option<Vec<_>> = None;
                for i in 0..self.types_of(list).len() {
                    let a = self.types_of(list)[i];
                    let x = m(self, a);
                    record(&mut changed, self.types_of(list), i, x);
                }
                let args = match changed {
                    None if mapped == c => return t,
                    None => self.types_of(list).to_vec(),
                    Some(args) => args,
                };
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

/// Records `x`, the mapped `i`th of `items`, in `changed`, which starts with the first item that
/// changed: a copy of the items before it.
fn record<T: Copy + PartialEq>(changed: &mut Option<Vec<T>>, items: &[T], i: usize, x: T) {
    match changed {
        Some(mapped) => mapped.push(x),
        None if x != items[i] => *changed = Some([&items[..i], &[x]].concat()),
        None => {}
    }
}
