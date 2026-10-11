//! Base types and member types as seen from a prefix (dotty's `baseType` and `asSeenFrom`).

use super::{Kind, SymbolId, Universe};
use crate::tags::{OBJECT, OPAQUE};
use crate::types::{Type, TypeId};

/// How many aliases and parents `base_type` follows.
const MAX_DEPTH: u32 = 64;

impl Universe {
    /// The type parameters of class or method `c`, in order.
    pub fn type_params(&self, c: SymbolId) -> Vec<SymbolId> {
        let decls = &self.symbol(c).decls;
        decls.iter().copied().filter(|&d| self.symbol(d).kind == Kind::TypeParam).collect()
    }

    /// The class type parameter reference `t` names: its class and index.
    pub fn class_param(&mut self, t: TypeId) -> Option<(SymbolId, usize)> {
        let s = match self.types().get(t).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(p, n) => self.resolve(p, &n, true)?,
            _ => return None,
        };
        let owner = self.symbol(s).owner?;
        if self.symbol(s).kind != Kind::TypeParam || self.symbol(owner).kind != Kind::Class {
            return None;
        }
        Some((owner, self.type_params(owner).iter().position(|&p| p == s)?))
    }

    /// `t` seen as an instance of class `base`: `base` applied to the arguments `t` passes it
    /// through its parents; `None` if `t` does not derive from `base`.
    pub fn base_type(&mut self, t: TypeId, base: SymbolId) -> Option<TypeId> {
        self.base_type_within(t, base, MAX_DEPTH)
    }

    fn base_type_within(&mut self, t: TypeId, base: SymbolId, depth: u32) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        let (tycon, args) = match self.types().get(t).clone() {
            Type::Applied(tycon, args) => (tycon, args),
            Type::And(a, b) => {
                return self
                    .base_type_within(a, base, depth)
                    .or_else(|| self.base_type_within(b, base, depth));
            }
            Type::Or(a, b) if self.is_bottom_ref(a) => return self.base_type_within(b, base, depth),
            Type::Or(a, b) if self.is_bottom_ref(b) => return self.base_type_within(a, base, depth),
            Type::Or(a, b) => {
                let x = self.base_type_within(a, base, depth)?;
                return (self.base_type_within(b, base, depth)? == x).then_some(x);
            }
            Type::This(u)
            | Type::Super(_, u)
            | Type::Annotated(u, _)
            | Type::Refined(u, _, _)
            | Type::Alias(u)
            | Type::Bounds(_, u) => {
                return self.base_type_within(u, base, depth);
            }
            _ => (t, Vec::new()),
        };
        let (prefix, s) = match self.types().get(tycon).clone() {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, n) => (Some(p), self.resolve(p, &n, true)?),
            _ => return None,
        };
        if self.symbol(s).flags.has(OPAQUE) {
            return None;
        }
        if self.symbol(s).kind != Kind::Class {
            let term = self.symbol(s).kind.is_term();
            let dealiased = match (args.is_empty(), prefix) {
                (true, Some(p)) if term => self.member_info(p, s),
                (true, _) => self.info(s),
                (false, _) => self.expand(tycon, &args)?,
            };
            let dealiased = match *self.types().get(dealiased) {
                Type::ByName(r) if term => r,
                _ => dealiased,
            };
            return self.base_type_within(dealiased, base, depth);
        }
        if s == base {
            return Some(t);
        }
        if !self.derives_from(s, base) {
            return None;
        }
        for parent in self.parents(s).into_iter().rev() {
            if !self.class_symbol(parent).is_some_and(|p| self.derives_from(p, base)) {
                continue;
            }
            let parent = self.subst_class_params(parent, s, &args);
            let parent = match (prefix, self.symbol(s).owner) {
                (Some(p), Some(o)) if self.symbol(o).kind == Kind::Class => self.as_seen_from(parent, p, o),
                _ => parent,
            };
            if let Some(found) = self.base_type_within(parent, base, depth) {
                return Some(found);
            }
        }
        None
    }

    /// Whether `t` refers to `Nothing` or `Null`.
    fn is_bottom_ref(&self, t: TypeId) -> bool {
        let b = self.builtins();
        matches!(*self.types().get(t), Type::Sym(_, s) if s == b.nothing || s == b.null)
    }

    /// `t` with the type parameters of class `c` replaced by `args`.
    fn subst_class_params(&mut self, t: TypeId, c: SymbolId, args: &[TypeId]) -> TypeId {
        if args.is_empty() {
            return t;
        }
        self.map_type(t, &mut |u, x| match u.class_param(x) {
            Some((owner, i)) if owner == c => args.get(i).copied(),
            _ => None,
        })
    }

    /// `t`, a type in class `owner`, as seen from `prefix`, from its `this` for a `super`: the
    /// `this` of `owner`, of its base classes and of its enclosing classes replaced by `prefix`
    /// or the prefix `prefix` gives them, the type parameters of `owner` by the arguments
    /// `prefix` gives them.
    pub fn as_seen_from(&mut self, t: TypeId, prefix: TypeId, owner: SymbolId) -> TypeId {
        let prefix = match *self.types().get(prefix) {
            Type::Super(this, _) => this,
            _ => prefix,
        };
        self.map_type(t, &mut |u, x| match u.types().get(x).clone() {
            Type::Sym(Some(_), s) if u.is_static_symbol(s) => Some(x),
            Type::This(c) => {
                let c = u.class_symbol(c)?;
                u.this_seen_from(prefix, owner, c, MAX_DEPTH)
            }
            _ => {
                let (c, i) = u.class_param(x)?;
                if !u.derives_from(owner, c) {
                    return None;
                }
                let seen = u.base_type(prefix, c)?;
                match u.types().get(seen) {
                    Type::Applied(_, args) => args.get(i).copied(),
                    _ => None,
                }
            }
        })
    }

    /// Whether `s` is reached through static paths only: its owner is a package or an object
    /// that is itself static.
    pub fn is_static_symbol(&self, s: SymbolId) -> bool {
        match self.symbol(s).owner {
            None => true,
            Some(o) => {
                let owner = self.symbol(o);
                owner.kind == Kind::Package
                    || (owner.kind == Kind::Class && owner.flags.has(OBJECT) && self.is_static_symbol(o))
            }
        }
    }

    /// What `this_cls.this` in class `cls` is as seen from `pre`: `pre` if it is an instance of
    /// `this_cls`, else what it is as seen from the prefix of `pre`'s base type `cls`, in the
    /// class enclosing `cls`; `None` to keep it.
    fn this_seen_from(
        &mut self,
        pre: TypeId,
        cls: SymbolId,
        this_cls: SymbolId,
        depth: u32,
    ) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        let pre_kind = self.types().get(pre).clone();
        if self.symbol(cls).kind == Kind::Package || matches!(pre_kind, Type::NoPrefix) {
            return None;
        }
        let widened = self.widen(pre);
        if self.derives_from(this_cls, cls) && self.base_type(widened, this_cls).is_some() {
            return Some(pre);
        }
        if matches!(pre_kind, Type::Package(_) | Type::PackageName(_)) {
            return None;
        }
        let base = self.base_type(widened, cls)?;
        let outer = self.normalized_prefix(base)?;
        let owner = self.symbol(cls).owner?;
        self.this_seen_from(outer, owner, this_cls, depth)
    }

    /// The prefix of class type `t`, through applications.
    fn normalized_prefix(&mut self, t: TypeId) -> Option<TypeId> {
        match self.types().get(t).clone() {
            Type::Sym(p, _) => p,
            Type::TypeRef(p, _) | Type::TermRef(p, _) => Some(p),
            Type::Applied(tycon, _) => self.normalized_prefix(tycon),
            _ => None,
        }
    }

    /// The type of member `m` of `prefix`.
    pub fn member_info(&mut self, prefix: TypeId, m: SymbolId) -> TypeId {
        let info = self.info(m);
        match self.symbol(m).owner {
            Some(owner) if self.symbol(owner).kind == Kind::Class => self.as_seen_from(info, prefix, owner),
            _ => info,
        }
    }
}
