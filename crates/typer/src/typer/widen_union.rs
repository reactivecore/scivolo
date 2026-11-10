// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Unions of inferred types joined or hardened (dotty's `widenUnion`, `hardenUnions`).

use super::Typer;
use super::type_ops::TypeOps;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// `t` with top-level soft unions joined, hard ones simplified, type variables constrained to
    /// simplify them, under explicit nulls a top-level `| Null` kept (dotty's `widenUnion`).
    pub fn widen_union(&mut self, t: TypeId) -> Result<TypeId, &'static str> {
        let w = self.types().widened(t);
        if !self.explicit_nulls || !matches!(self.u.types().get(w), Type::Or(..)) {
            return self.widen_union_without_null(t);
        }
        let stripped = self.strip_null(w);
        if stripped == w {
            return self.widen_union_without_null(w);
        }
        let wide = self.widen_union_without_null(stripped)?;
        let d = self.u.dealias(wide);
        let Type::Or(_, _, soft) = *self.u.types().get(w) else { unreachable!() };
        if matches!(*self.u.types().get(d), Type::Sym(_, s) if s == self.u.builtins().any) {
            return Ok(wide);
        }
        let null = self.u.intern_type(Type::Sym(None, self.u.builtins().null));
        Ok(self.u.intern_type(Type::Or(wide, null, soft)))
    }

    fn widen_union_without_null(&mut self, t: TypeId) -> Result<TypeId, &'static str> {
        let t = self.types().widened(t);
        Ok(match self.u.types().get(t).clone() {
            Type::And(a, b) => {
                let (wa, wb) = (self.widen_union_without_null(a)?, self.widen_union_without_null(b)?);
                if (wa, wb) == (a, b) { t } else { self.u.intern_type(Type::And(wa, wb)) }
            }
            Type::Refined(parent, name, info) => {
                let wide = self.widen_union(parent)?;
                if wide == parent { t } else { self.u.intern_type(Type::Refined(wide, name, info)) }
            }
            Type::Rec(binder, parent) => {
                let wide = self.widen_union(parent)?;
                if wide == parent { t } else { self.u.intern_type(Type::Rec(binder, wide)) }
            }
            Type::Lambda { binder, params, result } => {
                let wide = self.widen_union(result)?;
                if wide == result {
                    t
                } else {
                    self.u.intern_type(Type::Lambda { binder, params, result: wide })
                }
            }
            Type::Or(a, b, soft) => {
                let (wa, wb) = (self.widen_union_without_null(a)?, self.widen_union_without_null(b)?);
                let union =
                    if soft { self.types().constraining_lub(wa, wb) } else { self.u.lub_as(wa, wb, false) };
                match *self.u.types().get(union) {
                    Type::Or(..) if soft => self.types().join(union)?,
                    _ => union,
                }
            }
            Type::Flexible(hi) => {
                let wide = self.widen_union_without_null(hi)?;
                self.u.flexible(wide)
            }
            _ => t,
        })
    }
}

impl TypeOps<'_> {
    /// Drops the `Null` parts of the top-level unions of `t`, widened and dealiased, also inside
    /// intersections and flexible types; `t` itself if there are none (dotty's `stripNull`).
    pub fn strip_null(&mut self, t: TypeId) -> TypeId {
        let widened = self.u.widen(t);
        let w = self.u.dealias(widened);
        let stripped = match *self.u.types().get(w) {
            Type::Or(a, b, soft) => {
                let (sa, sb) = (self.strip_null(a), self.strip_null(b));
                match (self.is_null_type(sa), self.is_null_type(sb)) {
                    (_, true) => sa,
                    (true, _) => sb,
                    _ if (sa, sb) == (a, b) => w,
                    _ => self.u.intern_type(Type::Or(sa, sb, soft)),
                }
            }
            Type::And(a, b) => {
                let (sa, sb) = (self.strip_null(a), self.strip_null(b));
                if sa != a && sb != b { self.u.intern_type(Type::And(sa, sb)) } else { w }
            }
            Type::Flexible(hi) => {
                let stripped = self.strip_null(hi);
                self.u.flexible(stripped)
            }
            _ => w,
        };
        if stripped != w { stripped } else { t }
    }

    /// Checks whether `t` is a reference to `Null`, dealiased (dotty's `isNullType`).
    pub fn is_null_type(&mut self, t: TypeId) -> bool {
        let d = self.u.dealias(t);
        self.u.is_null_ref(d)
    }

    /// The join of union `t`; an error where it is not known or intersects a function class.
    fn join(&mut self, t: TypeId) -> Result<TypeId, &'static str> {
        let joined = self.u.join(t).ok_or("union widening")?;
        if !matches!(self.u.types().get(joined), Type::And(..)) {
            return Ok(joined);
        }
        let mut parts = vec![joined];
        while let Some(p) = parts.pop() {
            let tycon = match *self.u.types().get(p) {
                Type::And(a, b) => {
                    parts.extend([a, b]);
                    continue;
                }
                Type::Applied(tycon, _) => tycon,
                _ => continue,
            };
            if self.u.class_symbol(tycon).is_some_and(|c| self.u.full_name(c).starts_with("scala.Function")) {
                return Err("union widening to a function class");
            }
        }
        Ok(joined)
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

    /// Makes the top-level unions of `t` hard, also inside intersections, refinements and type
    /// lambda results (dotty's `hardenUnions`).
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
            Type::Rec(binder, parent) => {
                let hard = self.harden_unions(parent);
                self.u.intern_type(Type::Rec(binder, hard))
            }
            Type::Lambda { binder, params, result } => {
                let hard = self.harden_unions(result);
                self.u.intern_type(Type::Lambda { binder, params, result: hard })
            }
            Type::Or(a, b, _) => {
                let (ha, hb) = (self.harden_unions(a), self.harden_unions(b));
                self.u.intern_type(Type::Or(ha, hb, false))
            }
            Type::Flexible(hi) => {
                let hard = self.harden_unions(hi);
                self.u.flexible(hard)
            }
            _ => t,
        }
    }
}
