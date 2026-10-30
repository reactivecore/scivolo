// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! What named types and paths refer to, as the comparison sees them.

use super::Cmp;
use super::meaning::{meaning, refined_meaning};
use crate::tags::OBJECT;
use crate::types::{Type, TypeId};
use crate::{Kind, NOTHING, SymbolId, Universe, is_package_object_name, module_class_name};

/// What a named type stands for.
#[derive(Clone, PartialEq, Debug)]
pub enum Meaning {
    Class(SymbolId),
    /// An alias, its right-hand side seen from the reference's prefix.
    Alias(TypeId),
    /// An abstract type, opaque type or type parameter, with its bounds; type lambdas for a
    /// higher-kinded one.
    Abstract(TypeId, TypeId),
    Term(SymbolId),
    /// A name the universe does not define, qualified.
    Unresolved(String),
    /// A name selected from a path that is neither resolved nor qualified.
    Unknown,
}

/// A named type: its symbol, its prefix and what it stands for.
pub struct Named {
    pub sym: Option<SymbolId>,
    pub prefix: Option<TypeId>,
    pub meaning: Meaning,
}

impl Named {
    /// Whether this and `other` name the same unresolved qualified name.
    pub fn same_unresolved(&self, other: &Named) -> bool {
        matches!(&self.meaning, Meaning::Unresolved(_)) && self.meaning == other.meaning
    }
}

impl Cmp<'_> {
    /// What `t` names, if it is a named type.
    pub fn named(&mut self, t: TypeId) -> Option<Named> {
        let u = &mut *self.u;
        let (prefix, name) = match u.types().get(t).clone() {
            Type::TypeRef(p, n) | Type::TermRef(p, n) => (p, n),
            Type::Sym(p, s) => return Some(Named { sym: Some(s), prefix: p, meaning: meaning(u, s, p) }),
            _ => return None,
        };
        Some(match u.resolve_ref(t) {
            Some(s) => Named { sym: Some(s), prefix: Some(prefix), meaning: meaning(u, s, Some(prefix)) },
            None => {
                let typeref = matches!(u.types().get(t), Type::TypeRef(..));
                let meaning = match typeref.then(|| refined_meaning(u, prefix, &name)).flatten() {
                    Some(m) => m,
                    None => u.unresolved_class(prefix, &name).map_or(Meaning::Unknown, Meaning::Unresolved),
                };
                Named { sym: None, prefix: Some(prefix), meaning }
            }
        })
    }

    /// Whether `t`, widened, is `Nothing` or erroneous.
    pub fn is_bottom(&mut self, t: TypeId) -> bool {
        let t = self.u.widen(t);
        let t = match *self.u.types().get(t) {
            Type::ByName(r) => r,
            _ => t,
        };
        let nothing = Some(self.u.builtins().nothing);
        match *self.u.types().get(t) {
            Type::Error => true,
            Type::Sym(_, s) => Some(s) == nothing,
            Type::TypeRef(..) => self.u.resolve_ref(t) == nothing,
            _ => t == NOTHING,
        }
    }

    /// Whether `t` is the type of values or a type lambda: not a method type or bounds.
    pub fn is_value_type(&mut self, t: TypeId) -> bool {
        !matches!(
            self.u.types().get(t),
            Type::Method { .. } | Type::Poly { .. } | Type::Bounds(..) | Type::ByName(_)
        )
    }

    /// Whether `t` is a path or a constant.
    pub fn is_stable(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::TermRef(..)
            | Type::This(_)
            | Type::Constant(..)
            | Type::Str(_)
            | Type::RecThis(_)
            | Type::Skolem(..) => true,
            Type::Sym(_, s) => matches!(self.u.symbol(s).kind, Kind::Val | Kind::Param),
            _ => false,
        }
    }

    /// The symbol a path or a `this` refers to.
    pub fn path_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match *self.u.types().get(t) {
            Type::Package(p) => Some(p),
            Type::This(c) => self.path_symbol(c),
            _ => self.named(t)?.sym,
        }
    }

    /// Whether `t` is a path whose value does not depend on another: a package, a `this`, or a
    /// top-level object.
    fn is_static_path(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Package(_) | Type::PackageName(_) | Type::This(_) => true,
            Type::TermRef(..) | Type::Sym(..) => self.named(t).and_then(|n| n.sym).is_some_and(|s| {
                let owner = self.u.symbol(s).owner;
                self.u.symbol(s).kind == Kind::Package
                    || self.u.symbol(s).flags.has(OBJECT)
                        && owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Package)
            }),
            _ => false,
        }
    }

    /// Whether prefixes `p1` and `p2` of references to `s` relate, a missing `p1` also as the
    /// `this` of the class owning `s`.
    pub fn sub_prefix_of(&mut self, s: SymbolId, p1: Option<TypeId>, p2: Option<TypeId>) -> bool {
        if self.sub_prefix(p1, p2) {
            return true;
        }
        let owner = self.u.symbol(s).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class);
        let (None, Some(owner)) = (p1, owner) else { return false };
        let class = self.u.intern_type(Type::Sym(None, owner));
        let this = self.u.intern_type(Type::This(class));
        self.sub_prefix(Some(this), p2)
    }

    /// Whether prefix `p1` of a reference relates to prefix `p2` of the same symbol's
    /// reference (dotty's `isSubPrefix`): static paths to the same symbol, a package and its
    /// package object, `this` of classes deriving from each other, or `p1 <: p2`; a missing
    /// prefix matches a static one.
    pub fn sub_prefix(&mut self, p1: Option<TypeId>, p2: Option<TypeId>) -> bool {
        let (p1, p2) = match (p1, p2) {
            (None, None) => return true,
            (None, Some(p)) | (Some(p), None) => return self.is_static_path(p),
            (Some(p1), Some(p2)) => (p1, p2),
        };
        if p1 == p2 {
            return true;
        }
        let static_paths = self.is_static_path(p1) && self.is_static_path(p2);
        if static_paths && let (Some(s1), Some(s2)) = (self.path_symbol(p1), self.path_symbol(p2)) {
            let u = &mut *self.u;
            if s1 == s2 || package_object_of(u, s1, s2) || package_object_of(u, s2, s1) {
                return true;
            }
            let this =
                matches!(u.types().get(p1), Type::This(_)) && matches!(u.types().get(p2), Type::This(_));
            let classes = u.symbol(s1).kind == Kind::Class && u.symbol(s2).kind == Kind::Class;
            if this && classes && u.derives_from(s1, s2) && u.derives_from(s2, s1) {
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
        && (is_package_object_name(name) || is_package_object_name(&module_class_name(name)))
}
