//! What named types and paths refer to, as the comparison sees them.

use super::Cmp;
use crate::tags::{NULLconst, OBJECT};
use crate::types::{Type, TypeId};
use crate::{ANY, Kind, NOTHING, SymbolId, Universe};

/// What a named type stands for.
#[derive(Clone, PartialEq, Debug)]
pub enum Meaning {
    Class(SymbolId),
    /// An alias, its right-hand side seen from the reference's prefix.
    Alias(TypeId),
    /// An abstract type, opaque type or type parameter, with its bounds.
    Abstract(TypeId, TypeId),
    Term(SymbolId),
    /// A name the universe does not define, qualified.
    Unresolved(String),
}

/// A named type: its symbol, its prefix and what it stands for.
pub struct Named {
    pub sym: Option<SymbolId>,
    pub prefix: Option<TypeId>,
    pub meaning: Meaning,
}

impl Cmp<'_> {
    /// What `t` names, if it is a named type.
    pub fn named(&mut self, t: TypeId) -> Option<Named> {
        let u = &mut *self.u;
        let (prefix, name, ty) = match u.types().get(t).clone() {
            Type::TypeRef(p, n) => (p, n, true),
            Type::TermRef(p, n) => (p, n, false),
            Type::Sym(p, s) => return Some(Named { sym: Some(s), prefix: p, meaning: meaning(u, s, p) }),
            _ => return None,
        };
        Some(match u.resolve(prefix, &name, ty) {
            Some(s) => Named { sym: Some(s), prefix: Some(prefix), meaning: meaning(u, s, Some(prefix)) },
            None => {
                let full = u.unresolved_class(prefix, &name).unwrap_or(name);
                Named { sym: None, prefix: Some(prefix), meaning: Meaning::Unresolved(full) }
            }
        })
    }

    /// Whether `t` is `Nothing` or erroneous.
    pub fn is_bottom(&mut self, t: TypeId) -> bool {
        t == NOTHING || matches!(self.u.types().get(t), Type::Error)
    }

    /// Whether `t` is a path or a constant.
    pub fn is_stable(&mut self, t: TypeId) -> bool {
        match self.u.types().get(t).clone() {
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::RecThis(_) => true,
            Type::Sym(_, s) => matches!(self.u.symbol(s).kind, Kind::Val | Kind::Param),
            _ => false,
        }
    }

    /// Whether `null` is a value of `t` (dotty's `isNullable`).
    pub fn nullable(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(t).map(|n| n.meaning) {
                Some(Meaning::Class(c)) => self.nullable_class(c),
                Some(Meaning::Term(_)) => {
                    let w = self.u.widen(t);
                    w != t && self.nullable(w)
                }
                _ => false,
            },
            Type::This(_) => {
                let w = self.u.widen(t);
                w != t && self.nullable(w)
            }
            Type::Refined(p, ..) | Type::Rec(_, p) | Type::Annotated(p) | Type::Applied(p, _) => {
                self.nullable(p)
            }
            Type::And(a, b) => self.nullable(a) && self.nullable(b),
            Type::Or(a, b) => self.nullable(a) || self.nullable(b),
            Type::Constant(tag, _) => tag == NULLconst,
            _ => false,
        }
    }

    /// Whether class `c` has `null` among its values: not `Nothing`, a value class or an object.
    fn nullable_class(&mut self, c: SymbolId) -> bool {
        let any_val = self.u.class_ref("scala.AnyVal");
        let any_val = self.u.class_symbol(any_val);
        c != self.u.builtins().nothing
            && !self.u.symbol(c).flags.has(OBJECT)
            && !any_val.is_some_and(|v| self.u.derives_from(c, v))
    }

    /// The symbol a path or a `this` refers to.
    pub fn path_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match self.u.types().get(t).clone() {
            Type::Package(p) => Some(p),
            Type::This(c) => self.path_symbol(c),
            _ => self.named(t)?.sym,
        }
    }

    /// Whether prefix `p1` of a reference relates to prefix `p2` of the same symbol's
    /// reference (dotty's `isSubPrefix`): no prefix, the same symbol, a package and its package
    /// object, classes deriving from each other, or `p1 <: p2`.
    pub fn sub_prefix(&mut self, p1: Option<TypeId>, p2: Option<TypeId>) -> bool {
        let (Some(p1), Some(p2)) = (p1, p2) else { return true };
        if p1 == p2 {
            return true;
        }
        if let (Some(s1), Some(s2)) = (self.path_symbol(p1), self.path_symbol(p2)) {
            let u = &mut *self.u;
            if s1 == s2 || package_object_of(u, s1, s2) || package_object_of(u, s2, s1) {
                return true;
            }
            let classes = u.symbol(s1).kind == Kind::Class && u.symbol(s2).kind == Kind::Class;
            if classes && u.derives_from(s1, s2) && u.derives_from(s2, s1) {
                return true;
            }
        }
        self.recur(p1, p2)
    }
}

/// Whether `obj` is a package object (class or value) of package `pkg`.
fn package_object_of(u: &Universe, obj: SymbolId, pkg: SymbolId) -> bool {
    let name = &u.symbol(obj).name;
    u.symbol(pkg).kind == Kind::Package
        && u.symbol(obj).owner == Some(pkg)
        && (name == "package"
            || name == "package$"
            || name.ends_with("$package")
            || name.ends_with("$package$"))
}

/// What symbol `s` stands for when referred to through `prefix`.
fn meaning(u: &mut Universe, s: SymbolId, prefix: Option<TypeId>) -> Meaning {
    let seen = |u: &mut Universe, t: TypeId| match (prefix, u.symbol(s).owner) {
        (Some(p), Some(owner)) if u.symbol(owner).kind == Kind::Class => u.as_seen_from(t, p, owner),
        _ => t,
    };
    match u.symbol(s).kind {
        Kind::Class => Meaning::Class(s),
        Kind::Type | Kind::TypeParam => {
            if let Some(alias) = u.alias_of(s) {
                return Meaning::Alias(seen(u, alias));
            }
            let info = u.info(s);
            match u.types().get(info).clone() {
                Type::Bounds(lo, hi) => Meaning::Abstract(seen(u, lo), seen(u, hi)),
                _ => Meaning::Abstract(NOTHING, ANY),
            }
        }
        _ => Meaning::Term(s),
    }
}
