//! Base types and member types as seen from a prefix (dotty's `baseType` and `asSeenFrom`).

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

/// How many aliases and parents `base_type` follows.
const MAX_DEPTH: u32 = 64;

impl Universe {
    /// The type parameters of class `c`, in order.
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
            Type::This(u)
            | Type::Annotated(u)
            | Type::Refined(u, _, _)
            | Type::Alias(u)
            | Type::Bounds(_, u) => {
                return self.base_type_within(u, base, depth);
            }
            _ => (t, Vec::new()),
        };
        let s = match self.types().get(tycon).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(p, n) => self.resolve(p, &n, true)?,
            _ => return None,
        };
        if self.symbol(s).kind != Kind::Class {
            let dealiased = match args.is_empty() {
                true => self.info(s),
                false => self.expand(tycon, &args)?,
            };
            return self.base_type_within(dealiased, base, depth);
        }
        if s == base {
            return Some(t);
        }
        for parent in self.parents(s).into_iter().rev() {
            let parent = self.subst_class_params(parent, s, &args);
            if let Some(found) = self.base_type_within(parent, base, depth) {
                return Some(found);
            }
        }
        None
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

    /// `t`, a type in class `owner`, as seen from `prefix`: `this` of `owner` and its base
    /// classes replaced by `prefix`, their type parameters by the arguments `prefix` gives them.
    pub fn as_seen_from(&mut self, t: TypeId, prefix: TypeId, owner: SymbolId) -> TypeId {
        self.map_type(t, &mut |u, x| match u.types().get(x).clone() {
            Type::This(c) => {
                let c = u.class_symbol(c)?;
                u.derives_from(owner, c).then_some(prefix)
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

    /// The type of member `m` of `prefix`.
    pub fn member_info(&mut self, prefix: TypeId, m: SymbolId) -> TypeId {
        let info = self.info(m);
        match self.symbol(m).owner {
            Some(owner) if self.symbol(owner).kind == Kind::Class => self.as_seen_from(info, prefix, owner),
            _ => info,
        }
    }
}
