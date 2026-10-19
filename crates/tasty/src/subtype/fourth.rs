// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The last try of a comparison (dotty's `fourthTry`), by the left-hand side: abstract types
//! through their upper bound, `Nothing` and `Null`, singletons, intersections.

use super::Cmp;
use super::refs::Meaning;
use crate::tags::OBJECT;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId};

impl Cmp<'_> {
    pub fn fourth_try(&mut self, a: TypeId, b: TypeId) -> bool {
        match self.u.types().get(a).clone() {
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(a).map(|n| n.meaning) {
                Some(Meaning::Abstract(_, hi)) => self.recur(hi, b) || self.lifted_left(a, b),
                Some(Meaning::Class(c)) if c == self.u.builtins().nothing => self.is_value_type(b),
                Some(Meaning::Class(c)) if c == self.u.builtins().null => self.nullable(b),
                Some(Meaning::Term(_)) => self.below_singleton(a, b),
                _ => false,
            },
            Type::Applied(tycon, args) => self.compare_applied1(a, b, tycon, &args),
            Type::Skolem(x, _) if matches!(*self.u.types().get(b), Type::Skolem(y, _) if x == y) => true,
            Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::RecThis(_) | Type::Skolem(..) => {
                self.below_singleton(a, b)
            }
            Type::Refined(parent, ..)
            | Type::Rec(_, parent)
            | Type::Annotated(parent, _)
            | Type::Match { bound: parent, .. } => self.recur(parent, b),
            Type::And(x, y) => self.and_below(a, b, x, y),
            _ => false,
        }
    }

    /// `x & y <: b`: through `b` dealiased, `|` in the intersection distributed, else one of
    /// the parts.
    fn and_below(&mut self, a: TypeId, b: TypeId, x: TypeId, y: TypeId) -> bool {
        let dealiased = self.u.dealias(b);
        if dealiased != b {
            return self.recur(a, dealiased);
        }
        for (or, other, left) in [(x, y, true), (y, x, false)] {
            if let Type::Or(p, q, _) = self.u.types().get(or).clone() {
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
        let path = matches!(self.u.types().get(b), Type::TermRef(..) | Type::This(_) | Type::Sym(..));
        let atom = path && self.is_singleton(b) && self.compare_atoms(a, b, true) == Some(true);
        if atom || path && self.same_path(a, b) {
            return true;
        }
        if let Some(n) = self.named(a)
            && let Some(s) = n.sym
            && matches!(self.u.symbol(s).kind, Kind::Val | Kind::Param | Kind::Def)
        {
            let underlying = self.u.term_type(n.prefix, s);
            let underlying = match *self.u.types().get(underlying) {
                Type::ByName(r) => r,
                _ => underlying,
            };
            let singleton = matches!(
                self.u.types().get(underlying),
                Type::TermRef(..) | Type::Sym(..) | Type::This(_) | Type::Constant(..) | Type::Str(_)
            );
            if singleton && underlying != a && self.recur(underlying, b) {
                return true;
            }
        }
        let widened = self.u.widen(a);
        widened != a && self.recur(widened, b)
    }

    /// Whether paths `a` and `b` denote the same value: the same term through related prefixes,
    /// `this` of the same class, or a top-level object and `this` of its class.
    fn same_path(&mut self, a: TypeId, b: TypeId) -> bool {
        let this = |s: &mut Self, t| match s.u.types().get(t).clone() {
            Type::This(c) => s.path_symbol(c),
            _ => None,
        };
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
