//! Unions of inferred types joined or hardened (dotty's `widenUnion`, `hardenUnions`).

use super::Typer;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// `t` widened, a by-name type as its result.
    pub fn widened(&mut self, t: TypeId) -> TypeId {
        let widened = self.u.widen(t);
        match *self.u.types().get(widened) {
            Type::ByName(r) => r,
            _ => widened,
        }
    }

    /// `t` with its top-level soft unions replaced by their joins, hard ones simplified, type
    /// variables constrained to simplify them (dotty's `widenUnion` without explicit nulls).
    pub fn widen_union(&mut self, t: TypeId) -> Result<TypeId, &'static str> {
        let t = self.widened(t);
        Ok(match self.u.types().get(t).clone() {
            Type::And(a, b) => {
                let (wa, wb) = (self.widen_union(a)?, self.widen_union(b)?);
                if (wa, wb) == (a, b) { t } else { self.u.intern_type(Type::And(wa, wb)) }
            }
            Type::Refined(parent, name, info) => {
                let wide = self.widen_union(parent)?;
                if wide == parent { t } else { self.u.intern_type(Type::Refined(wide, name, info)) }
            }
            Type::Or(a, b, soft) => {
                let (wa, wb) = (self.widen_union(a)?, self.widen_union(b)?);
                let union = if soft { self.constraining_lub(wa, wb) } else { self.u.lub_as(wa, wb, false) };
                match *self.u.types().get(union) {
                    Type::Or(..) if soft => self.join(union)?,
                    _ => union,
                }
            }
            _ => t,
        })
    }

    /// The join of union `t`; an error where it is not known, or where it has a function class
    /// among its intersected classes, whose arguments dotty may leave unsimplified.
    fn join(&mut self, t: TypeId) -> Result<TypeId, &'static str> {
        let joined = self.u.join(t).ok_or("union widening")?;
        let mut function = false;
        self.u.map_type(joined, &mut |u, part| {
            if let Type::Applied(tycon, _) = *u.types().get(part) {
                function |=
                    u.class_symbol(tycon).is_some_and(|c| u.full_name(c).starts_with("scala.Function"));
            }
            None
        });
        let several = matches!(self.u.types().get(joined), Type::And(..));
        if several && function { Err("union widening to a function class") } else { Ok(joined) }
    }

    /// The soft least upper bound of `a` and `b`, the parts of either that conform to a part of
    /// the other dropped, type variables constrained to make them conform (dotty's `lub` with
    /// `canConstrain`).
    fn constraining_lub(&mut self, a: TypeId, b: TypeId) -> TypeId {
        if self.is_fully_defined(a) && self.is_fully_defined(b) {
            return self.u.lub(a, b);
        }
        let (fa, fb) = (self.union_parts(a), self.union_parts(b));
        let fb: Vec<TypeId> =
            fb.into_iter().filter(|&x| !fa.iter().any(|&y| self.conforms_or_undo(x, y))).collect();
        let fa: Vec<TypeId> =
            fa.into_iter().filter(|&x| !fb.iter().any(|&y| self.conforms_or_undo(x, y))).collect();
        let parts = [fa, fb].concat();
        let first = parts[0];
        parts[1..].iter().fold(first, |acc, &p| self.u.union(acc, p, true))
    }

    fn union_parts(&self, t: TypeId) -> Vec<TypeId> {
        match *self.u.types().get(t) {
            Type::Or(a, b, _) => [self.union_parts(a), self.union_parts(b)].concat(),
            _ => vec![t],
        }
    }

    /// Whether `a` conforms to `b`, the constraint unchanged if not.
    fn conforms_or_undo(&mut self, a: TypeId, b: TypeId) -> bool {
        let mark = self.constraint.mark();
        let ok = self.conforms(a, b);
        if !ok {
            self.constraint.rollback(mark);
        }
        ok
    }

    /// `t` with its top-level unions made hard (dotty's `hardenUnions`).
    pub fn harden_unions(&mut self, t: TypeId) -> TypeId {
        let widened = self.widened(t);
        match self.u.types().get(widened).clone() {
            Type::And(a, b) => {
                let (ha, hb) = (self.harden_unions(a), self.harden_unions(b));
                self.u.intern_type(Type::And(ha, hb))
            }
            Type::Refined(parent, name, info) => {
                let hard = self.harden_unions(parent);
                self.u.intern_type(Type::Refined(hard, name, info))
            }
            Type::Or(a, b, _) => {
                let (ha, hb) = (self.harden_unions(a), self.harden_unions(b));
                self.u.intern_type(Type::Or(ha, hb, false))
            }
            _ => t,
        }
    }
}
