// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Base types of a type as instances of its base classes (dotty's `baseType`).

use super::{Kind, SymbolId, Universe};
use crate::tags::OPAQUE;
use crate::types::{Type, TypeId};

/// How many aliases and parents `base_type` follows.
pub const MAX_DEPTH: u32 = 64;

impl Universe {
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
            Type::Or(a, b, _) if self.is_bottom_ref(a) => return self.base_type_within(b, base, depth),
            Type::Or(a, b, _) if self.is_bottom_ref(b) => return self.base_type_within(a, base, depth),
            Type::Or(a, b, _) => {
                let x = self.base_type_within(a, base, depth)?;
                return (self.base_type_within(b, base, depth)? == x).then_some(x);
            }
            Type::This(u)
            | Type::Super(_, u)
            | Type::Annotated(u, _)
            | Type::Refined(u, _, _)
            | Type::Alias(u)
            | Type::Bounds(_, u)
            | Type::Skolem(_, u) => {
                return self.base_type_within(u, base, depth);
            }
            Type::Match { bound, .. } => {
                let sup = self.try_normalize(t).unwrap_or(bound);
                return self.base_type_within(sup, base, depth);
            }
            _ => (t, Vec::new()),
        };
        let (prefix, s) = match self.types().get(tycon).clone() {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, _) => (Some(p), self.resolve_ref(tycon)?),
            _ => return None,
        };
        if self.symbol(s).kind == Kind::Type && self.symbol(s).flags.has(OPAQUE) {
            let underlying = self.opaque_underlying(s, prefix)?;
            let applied = match (args.is_empty(), self.types().get(underlying)) {
                (true, _) => underlying,
                (false, Type::Lambda { .. }) => {
                    let applied = self.intern_type(Type::Applied(underlying, args));
                    self.beta_reduced(applied)
                }
                _ => return None,
            };
            return self.base_type_within(applied, base, depth);
        }
        if self.symbol(s).kind != Kind::Class {
            let term = self.symbol(s).kind.is_term();
            let dealiased = match (args.is_empty(), prefix) {
                (true, Some(p)) if term => self.member_info(p, s),
                (true, Some(p)) => {
                    let info = self.info(s);
                    match self.symbol(s).owner {
                        Some(o) if self.symbol(o).kind == Kind::Class => self.seen_from_path(info, p, o),
                        _ => info,
                    }
                }
                (true, None) => self.info(s),
                (false, _) => self.expand_upper(tycon, &args)?,
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
        let join = self.parents_disagree(s, base);
        let mut joined = None;
        for parent in self.parents(s).into_iter().rev() {
            if !self.class_symbol(parent).is_some_and(|p| self.derives_from(p, base)) {
                continue;
            }
            let parent = self.subst_class_params(parent, s, &args);
            let parent = match (prefix, self.symbol(s).owner) {
                (Some(p), Some(o)) if self.symbol(o).kind == Kind::Class => self.as_seen_from(parent, p, o),
                _ => parent,
            };
            let Some(found) = self.base_type_within(parent, base, depth) else { continue };
            if !join {
                return Some(found);
            }
            joined = Some(match joined {
                Some(j) => self.joined_base_type(base, j, found).unwrap_or(j),
                None => found,
            });
        }
        joined
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
}
