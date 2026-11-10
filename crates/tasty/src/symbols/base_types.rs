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
        let (tycon, args) = match self.ty(t).clone() {
            Type::Applied(tycon, args) => (tycon, args),
            Type::And(a, b) => {
                return match (self.base_type_within(a, base, depth), self.base_type_within(b, base, depth)) {
                    (Some(x), Some(y)) => self.merged_bases(t, base, (x, y), true),
                    (x, y) => x.or(y),
                };
            }
            Type::Or(a, b, _) if self.is_bottom_ref(a) => return self.base_type_within(b, base, depth),
            Type::Or(a, b, _) if self.is_bottom_ref(b) => return self.base_type_within(a, base, depth),
            Type::Or(a, b, _) => {
                let x = self.base_type_within(a, base, depth)?;
                let y = self.base_type_within(b, base, depth)?;
                return self.merged_bases(t, base, (x, y), false);
            }
            Type::This(u)
                if let Some(c) = self.class_symbol(u)
                    && !self.derives_from(c, base)
                    && let Some(given) = self.given_self_type(c) =>
            {
                return self.base_type_within(given, base, depth);
            }
            Type::This(u)
            | Type::Super(_, u)
            | Type::Annotated(u, _)
            | Type::Flexible(u)
            | Type::Refined(u, _, _)
            | Type::Rec(_, u)
            | Type::Alias(u)
            | Type::Bounds(_, u)
            | Type::Skolem(_, u) => {
                return self.base_type_within(u, base, depth);
            }
            Type::Match { bound, .. } => {
                let sup = self.try_normalize(t).unwrap_or(bound);
                return self.base_type_within(sup, base, depth);
            }
            Type::TermRef(..) if self.resolve_ref(t).is_none() => {
                let underlying = self.widen(t);
                return (underlying != t).then(|| self.base_type_within(underlying, base, depth)).flatten();
            }
            _ => (t, Vec::new()),
        };
        let (prefix, s) = match *self.ty(tycon) {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, _) => (Some(p), self.resolve_ref(tycon)?),
            _ => return None,
        };
        if self.symbol(s).kind == Kind::Type && self.symbol(s).flags.has(OPAQUE) {
            let underlying = self.opaque_underlying(s, prefix)?;
            let applied = match (args.is_empty(), self.ty(underlying)) {
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
            if !term && !args.is_empty() {
                let dealiased = self.dealias(t);
                if dealiased != t {
                    return self.base_type_within(dealiased, base, depth);
                }
            }
            let dealiased = match (args.is_empty(), prefix) {
                (true, Some(p)) if term => self.member_info(p, s),
                (true, Some(p)) => {
                    let path = self.is_value_path(p);
                    let refined = if path { self.refinement_info(p, s) } else { None };
                    if let Some(refined) = refined.or_else(|| self.arg_for_param(p, s)) {
                        return self.base_type_within(refined, base, depth);
                    }
                    let info = self.info(s);
                    match self.symbol(s).owner {
                        Some(o) if path && self.symbol(o).kind == Kind::Class => {
                            self.as_seen_from(info, p, o)
                        }
                        _ => info,
                    }
                }
                (true, _) => self.info(s),
                (false, _) => self.expand_upper(tycon, &args)?,
            };
            let dealiased = self.by_name(dealiased).filter(|_| term).unwrap_or(dealiased);
            return self.base_type_within(dealiased, base, depth);
        }
        if s == base {
            return Some(t);
        }
        if !self.derives_from(s, base) || self.is_null_without(s, base) {
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

    /// The base types `x` and `y` of the parts of intersection (`and`) or union `t` merged by
    /// `glb` or `lub`; while that merge is in progress, `x` of an intersection, else `None`.
    fn merged_bases(
        &mut self,
        t: TypeId,
        base: SymbolId,
        (x, y): (TypeId, TypeId),
        and: bool,
    ) -> Option<TypeId> {
        if x == y {
            return Some(x);
        }
        if !self.merging_bases.insert((t, base)) {
            return and.then_some(x);
        }
        let merged = if and { self.glb(x, y) } else { self.lub(x, y) };
        self.merging_bases.remove(&(t, base));
        Some(merged)
    }

    /// Whether `t` refers to `Nothing`, or to `Null` without safe nulls.
    fn is_bottom_ref(&self, t: TypeId) -> bool {
        let b = self.defn();
        let safe = self.null_mode.safe;
        matches!(*self.ty(t), Type::Sym(_, s) if s == b.nothing || s == b.null && !safe)
    }

    /// Whether `c` is `Null` under explicit nulls, whose parents are `Any` and `Matchable`, and
    /// `base` is neither.
    fn is_null_without(&self, c: SymbolId, base: SymbolId) -> bool {
        let b = self.defn();
        self.null_mode.explicit && c == b.null && base != b.any && base != b.matchable
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
