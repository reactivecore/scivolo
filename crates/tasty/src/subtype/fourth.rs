// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The last try of a comparison (dotty's `fourthTry`), by the left-hand side: abstract types
//! through their upper bound, `Nothing` and `Null`, singletons, intersections.

use super::Cmp;
use super::refs::Meaning;
use crate::tags::OBJECT;
use crate::types::{Type, TypeId, TypeList};
use crate::{Kind, SymbolId};

impl Cmp<'_> {
    /// Decides `a <: b` by the shape of `a` once the earlier tries failed (dotty's `fourthTry`):
    /// - an abstract type through its upper bound or its GADT bound from a pattern match;
    /// - `Nothing` below every value type, `Null` below every nullable one;
    /// - singletons (`x.type`, `this`, literals) through their underlying type;
    /// - applied types, match types, refinements, intersections and type lambdas by their parts.
    pub fn fourth_try(&mut self, a: TypeId, b: TypeId) -> bool {
        match *self.u.ty(a) {
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(a).map(|n| n.meaning) {
                Some(Meaning::Abstract(_, hi)) => {
                    self.recur(hi, b)
                        || self.gadt_bounds(a).is_some_and(|(_, ghi)| self.recur_frozen(ghi, b))
                        || self.lifted_left(a, b)
                }
                Some(Meaning::Class(c)) if c == self.u.defn().nothing => self.is_value_type(b),
                Some(Meaning::Class(c)) if c == self.u.defn().null => self.nullable(b),
                Some(Meaning::Term(_)) => self.below_singleton(a, b),
                Some(Meaning::Class(c)) if self.u.is_module_class(c) => self.module_class_below(a, b, c),
                _ => false,
            },
            Type::Applied(tycon, args) => {
                let args = self.u.types_of(args).to_vec();
                self.compare_applied1(a, b, tycon, &args)
            }
            Type::Skolem(x, _) if matches!(*self.u.ty(b), Type::Skolem(y, _) if x == y) => true,
            Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::RecThis(_) | Type::Skolem(..) => {
                self.below_singleton(a, b)
            }
            Type::Match { bound, scrutinee, cases } => {
                self.recur(bound, b) || self.match_below(scrutinee, cases, b)
            }
            Type::Refined(parent, ..)
            | Type::Rec(_, parent)
            | Type::Annotated(parent, _)
            | Type::Flexible(parent) => self.recur(parent, b),
            Type::And(x, y) => self.and_below(a, b, x, y),
            Type::Lambda { binder, params, result } => {
                let n = self.u.params_of(params).len();
                self.lambda_below(binder, n, result, b)
            }
            _ => false,
        }
    }

    /// The GADT bounds of type parameter `t`, plain or seen from `this`, if it has some.
    pub fn gadt_bounds(&mut self, t: TypeId) -> Option<(TypeId, TypeId)> {
        match *self.u.ty(t) {
            Type::Sym(None, s) => self.u.gadt_bounds(s),
            Type::Sym(Some(p), s) => {
                let c = self.u.this_of(p)?;
                (self.u.class_symbol(c) == self.u.symbol(s).owner).then(|| self.u.gadt_bounds(s))?
            }
            _ => None,
        }
    }

    /// `scrutinee match { cases } <: b` for a match type `b` on the same scrutinee, `scrutinee`
    /// with its skolems widened, and cases below `b`'s (dotty's `compareMatch`).
    fn match_below(&mut self, mut scrutinee: TypeId, cases: TypeList, b: TypeId) -> bool {
        let Type::Match { scrutinee: other, cases: others, .. } = *self.u.ty(b) else {
            return false;
        };
        while !(self.recur(scrutinee, other) && self.recur(other, scrutinee)) {
            let Type::Skolem(_, widened) = *self.u.ty(scrutinee) else { return false };
            scrutinee = widened;
        }
        let (cases, others) = (self.u.types_of(cases).to_vec(), self.u.types_of(others).to_vec());
        cases.len() == others.len() && cases.iter().zip(&others).all(|(&x, &y)| self.recur(x, y))
    }

    /// `[X..] =>> result <: b` for a `b` that is no lambda: `result <: b[X..]` if `b` takes as
    /// many type parameters (dotty's `compareHKLambda`).
    fn lambda_below(&mut self, binder: crate::types::Binder, n: usize, result: TypeId, b: TypeId) -> bool {
        if self.u.lambda(b).is_some() || self.hk_arity(b) != Some(n) {
            return false;
        }
        let refs: Vec<TypeId> = (0..n as u32).map(|i| self.u.intern_type(Type::Param(binder, i))).collect();
        let applied = self.apply(b, &refs);
        self.recur(result, applied)
    }

    /// `a <: b` for `a` the type of module class `c`: `b` a reference to `c`'s object through a
    /// related prefix (dotty's `compareNamed`).
    fn module_class_below(&mut self, a: TypeId, b: TypeId, c: SymbolId) -> bool {
        if !matches!(self.u.ty(b), Type::TermRef(..) | Type::Sym(..)) {
            return false;
        }
        let (Some(na), Some(nb)) = (self.named(a), self.named(b)) else { return false };
        nb.sym.is_some() && nb.sym == self.u.source_module(c) && self.sub_prefix(na.prefix, nb.prefix)
    }

    /// `x & y <: b`: through `b` dealiased, `|` in the intersection distributed, else one of
    /// the parts.
    fn and_below(&mut self, a: TypeId, b: TypeId, x: TypeId, y: TypeId) -> bool {
        let dealiased = self.u.dealias(b);
        if dealiased != b {
            return self.recur(a, dealiased);
        }
        for (or, other, left) in [(x, y, true), (y, x, false)] {
            if let Type::Or(p, q, _) = *self.u.ty(or) {
                let inter = |s: &mut Self, t| match left {
                    true => s.u.intern_type(Type::And(t, other)),
                    false => s.u.intern_type(Type::And(other, t)),
                };
                let (ap, aq) = (inter(self, p), inter(self, q));
                return self.recur(ap, b) && self.recur(aq, b);
            }
        }
        self.either(|s| s.recur(x, b), |s| s.recur(y, b))
    }

    /// `a <: b` for a singleton `a`: the same atom or path, `a`'s underlying path (dotty's
    /// `underlying`), or `a` widened.
    fn below_singleton(&mut self, a: TypeId, b: TypeId) -> bool {
        let path = matches!(self.u.ty(b), Type::TermRef(..) | Type::This(_) | Type::Sym(..));
        let atom = path && self.is_singleton(b) && self.compare_atoms(a, b, true) == Some(true);
        if atom || path && self.same_path(a, b) {
            return true;
        }
        if path
            && let Some(underlying) = self.singleton_underlying(b)
            && self.recur(a, underlying)
        {
            return true;
        }
        if let Some(underlying) = self.singleton_underlying(a)
            && self.recur(underlying, b)
        {
            return true;
        }
        let widened = self.u.widen(a);
        widened != a && self.recur(widened, b)
    }

    /// The declared type of term reference `t` if it is a singleton other than `t`.
    fn singleton_underlying(&mut self, t: TypeId) -> Option<TypeId> {
        let n = self.named(t)?;
        let s = n.sym.filter(|&s| matches!(self.u.symbol(s).kind, Kind::Val | Kind::Param | Kind::Def))?;
        let name = self.u.interned_name(s);
        let refined = n.prefix.and_then(|p| self.u.term_refinement_named(p, name));
        let underlying = refined.unwrap_or_else(|| self.u.term_type(n.prefix, s));
        let underlying = self.u.by_name(underlying).unwrap_or(underlying);
        let singleton = match *self.u.ty(underlying) {
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => true,
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            _ => false,
        };
        (singleton && underlying != t).then_some(underlying)
    }

    /// Whether paths `a` and `b` denote the same value: the same term through related prefixes,
    /// `this` of the same class, or a top-level object and `this` of its class.
    fn same_path(&mut self, a: TypeId, b: TypeId) -> bool {
        let this = |s: &mut Self, t| s.u.this_of(t).and_then(|c| s.path_symbol(c));
        match (this(self, a), this(self, b)) {
            (Some(c1), Some(c2)) => c1 == c2,
            (Some(c), None) => self.object_of(b) == Some(c),
            (None, Some(c)) => self.object_of(a) == Some(c),
            (None, None) => {
                let (Some(n1), Some(n2)) = (self.named(a), self.named(b)) else { return false };
                n1.sym.is_some() && n1.sym == n2.sym && self.sub_prefix(n1.prefix, n2.prefix)
            }
        }
    }

    /// The class of the top-level object path `t` refers to.
    fn object_of(&mut self, t: TypeId) -> Option<SymbolId> {
        let s = self.named(t)?.sym?;
        let owner = self.u.symbol(s).owner?;
        let top_level = self.u.symbol(owner).kind == Kind::Package && self.u.symbol(s).flags.has(OBJECT);
        top_level.then(|| self.u.module_class(s)).flatten()
    }
}
