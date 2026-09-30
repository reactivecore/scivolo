//! The first two tries of a comparison (dotty's `firstTry`, by the right-hand side, and
//! `secondTry`, by the left), and named types on the right (`compareNamed`, `thirdTryNamed`).

use super::Cmp;
use super::refs::{Meaning, Named};
use crate::types::{Type, TypeId, VarId};
use crate::{Kind, SymbolId};

impl Cmp<'_> {
    pub fn first_try(&mut self, a: TypeId, b: TypeId) -> bool {
        if matches!(self.u.types().get(a), Type::Error) {
            return true;
        }
        match self.u.types().get(b).clone() {
            Type::Error => true,
            Type::Var(v) => match self.vars.instance(v) {
                Some(t) => self.recur(a, t),
                None => self.second_try(a, b),
            },
            Type::Wildcard(bounds) => match bounds.map(|t| self.u.types().get(t).clone()) {
                Some(Type::Bounds(_, hi)) => self.recur(a, hi),
                _ => true,
            },
            Type::Annotated(p) => self.recur(a, p),
            Type::This(c2) => match self.u.types().get(a).clone() {
                Type::This(c1) => {
                    self.path_symbol(c1).is_some() && self.path_symbol(c1) == self.path_symbol(c2)
                }
                _ => self.second_try(a, b),
            },
            Type::And(x, y) => self.recur(a, x) && self.recur(a, y),
            Type::Or(x, y) if x == y => self.recur(a, x),
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => self.compare_named(a, b),
            _ => self.second_try(a, b),
        }
    }

    fn compare_named(&mut self, a: TypeId, b: TypeId) -> bool {
        let Some(n2) = self.named(b) else { return self.second_try(a, b) };
        if let Meaning::Alias(alias) = n2.meaning
            && self.recur(a, alias)
        {
            return true;
        }
        let Some(n1) = self.named(a) else { return self.second_try(a, b) };
        if let Meaning::Alias(alias) = n1.meaning
            && self.recur(alias, b)
        {
            return true;
        }
        match (n1.sym, n2.sym) {
            (Some(s1), Some(s2)) if s1 == s2 => {
                self.owned_by_package(s1)
                    || self.sub_prefix(n1.prefix, n2.prefix)
                    || self.third_try_named(a, b, n2)
            }
            (None, None) if n1.same_unresolved(&n2) => true,
            _ => self.third_try_named(a, b, n2),
        }
    }

    /// Whether `s` is owned by a package.
    fn owned_by_package(&mut self, s: SymbolId) -> bool {
        self.u.symbol(s).owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Package)
    }

    pub fn second_try(&mut self, a: TypeId, b: TypeId) -> bool {
        match self.u.types().get(a).clone() {
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => {
                match self.named(a).map(|n| n.meaning) {
                    Some(Meaning::Alias(alias)) if self.recur(alias, b) => return true,
                    Some(Meaning::Class(c)) if c == self.u.builtins().nothing && self.is_value_type(b) => {
                        return true;
                    }
                    _ => {}
                }
                self.third_try(a, b)
            }
            Type::Var(v) => match self.vars.instance(v) {
                Some(t) => self.recur(t, b),
                None => self.var_below(a, v, b),
            },
            Type::Wildcard(bounds) => match bounds.map(|t| self.u.types().get(t).clone()) {
                Some(Type::Bounds(lo, _)) => self.recur(lo, b),
                _ => true,
            },
            Type::And(x, y) if x == y => self.recur(x, b),
            Type::Or(x, y) => self.recur(x, b) && self.recur(y, b),
            Type::Annotated(p) => self.recur(p, b),
            _ => self.third_try(a, b),
        }
    }

    /// `v <: b` for variable `a = v` without instance: through its upper bound, else by
    /// constraining it.
    fn var_below(&mut self, a: TypeId, v: VarId, b: TypeId) -> bool {
        let (_, hi) = self.vars.bounds(v);
        if self.recur_frozen(hi, b) {
            return true;
        }
        match self.frozen() {
            true => self.third_try(a, b),
            false => self.vars.add_upper(self.u, v, b),
        }
    }

    /// `a <: b` for a named type `b` that is not an alias of a supertype of `a`.
    pub fn third_try_named(&mut self, a: TypeId, b: TypeId, n2: Named) -> bool {
        match n2.meaning {
            Meaning::Abstract(lo, _) => self.recur(a, lo) || self.fourth_try(a, b),
            Meaning::Class(c) if self.u.type_params(c).is_empty() => {
                let builtins = self.u.builtins();
                if c == builtins.any_kind || self.is_bottom(a) {
                    return true;
                }
                if matches!(self.u.types().get(a), Type::Lambda { .. }) {
                    return false;
                }
                if c == builtins.any || c == builtins.singleton && self.is_stable(a) {
                    return true;
                }
                self.try_base_type(a, b, c)
            }
            _ => self.fourth_try(a, b),
        }
    }

    /// `a <: b` through the instance of class `c`, `b`'s class, that `a` derives.
    pub fn try_base_type(&mut self, a: TypeId, b: TypeId, c: SymbolId) -> bool {
        let widened = self.u.widen(a);
        match self.u.base_type(widened, c) {
            Some(base) if base != a => {
                let and = matches!(self.u.types().get(widened), Type::And(..));
                match and || matches!(self.u.types().get(base), Type::Or(..)) {
                    true => self.either(|s| s.recur(base, b), |s| s.fourth_try(a, b)),
                    false => self.recur(base, b),
                }
            }
            _ => self.fourth_try(a, b),
        }
    }
}
