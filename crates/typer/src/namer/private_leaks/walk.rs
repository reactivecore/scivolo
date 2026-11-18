// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! The map over a signature (dotty's `NotPrivate`).

use super::PrivateLeaks;
use crate::typer::Typer;
use tasty::tags::PRIVATE;
use tasty::{Kind, SymbolId, Type, TypeId};

impl PrivateLeaks {
    /// Maps `ty` to itself with the private aliases that leak expanded, where that avoids the
    /// leak; records the other leaks.
    pub fn apply(&mut self, t: &mut Typer<'_>, ty: TypeId) -> TypeId {
        let mut named = Vec::new();
        t.u.map_type(ty, &mut |u, x| match *u.ty(x) {
            Type::Sym(..) => {
                named.push(x);
                Some(x)
            }
            Type::This(_) => Some(x),
            _ => None,
        });
        let mapped: Vec<(TypeId, TypeId)> = named.into_iter().map(|x| (x, self.apply_named(t, x))).collect();
        if mapped.iter().all(|(x, y)| x == y) {
            return ty;
        }
        t.u.map_type(ty, &mut |u, x| match *u.ty(x) {
            Type::Sym(..) => mapped.iter().find(|m| m.0 == x).map(|m| m.1),
            Type::This(_) => Some(x),
            _ => None,
        })
    }

    /// Reference `x` with the leaks of its symbol or prefix avoided by expanding it if it is an
    /// alias.
    fn apply_named(&mut self, t: &mut Typer<'_>, x: TypeId) -> TypeId {
        let Some((prefix, s)) = t.u.sym_ref(x) else { return x };
        let before = self.leaks.len();
        let mut mapped = x;
        if self.is_leaked(t, s) {
            self.leaks.push(s);
        } else if let Some(p) = prefix {
            let q = self.apply(t, p);
            if q != p {
                mapped = t.u.intern_type(Type::Sym(Some(q), s));
            }
        }
        if self.leaks.len() == before || t.u.symbol(s).kind != Kind::Type || self.expanding.contains(&s) {
            return mapped;
        }
        t.complete(s);
        let Some(alias) = t.u.referenced_alias(x) else { return mapped };
        let found = self.leaks.split_off(before);
        self.expanding.push(s);
        let expanded = self.apply(t, alias);
        self.expanding.pop();
        if self.leaks.len() == before {
            return expanded;
        }
        self.leaks.truncate(before);
        self.leaks.extend(found);
        mapped
    }

    /// Whether `other`, which the signature refers to, is private to a class that the boundary
    /// lies outside of, its companion included.
    fn is_leaked(&self, t: &mut Typer<'_>, other: SymbolId) -> bool {
        let s = t.u.symbol(other);
        if !s.flags.has(PRIVATE) || s.kind == Kind::TypeParam {
            return false;
        }
        let Some(owner) = s.owner else { return false };
        !t.is_contained_in(self.boundary, owner)
            && !t.linked_class(owner).is_some_and(|linked| t.is_contained_in(self.boundary, linked))
    }
}
