// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The widening of inferred types (dotty's `widenInferred`): singletons widened unless
//! singleton-bounded, soft unions joined or hard unions hardened, module classes as their objects.

use super::Typer;
use super::type_ops::TypeOps;
use tasty::{Kind, Type, TypeId, TypeVars};

/// The widenings `widen_inferred` applies (dotty's `Widen`): of singletons only, unions hardened,
/// or also of soft unions to their joins.
#[derive(Clone, Copy, PartialEq)]
pub enum Widen {
    Singletons,
    Unions,
}

impl Typer<'_> {
    /// `inst` widened as an instance below `bound` (dotty's `widenInferred`); an error where the
    /// join of a soft union is not known.
    pub fn widen_inferred(
        &mut self,
        inst: TypeId,
        bound: TypeId,
        widen: Widen,
    ) -> Result<TypeId, &'static str> {
        if self.is_below_singleton(bound, 0) {
            return Ok(self.without_repeated_annot(inst));
        }
        let single = self.widen_single(inst, bound, widen == Widen::Unions);
        let wide = match widen {
            Widen::Unions => {
                let union = self.widen_union(single)?;
                if union == single {
                    single
                } else if self.is_transparent(union, false) {
                    self.widen_single(single, bound, false)
                } else if self.conforms(union, bound) {
                    union
                } else {
                    single
                }
            }
            Widen::Singletons => self.types().harden_unions(single),
        };
        let wide = self.without_transparent_traits(wide, bound);
        let module = self.types().module_ref(wide);
        let wide = match self.u.sym_ref(inst) {
            Some((Some(p), s)) if self.u.sym_of(module) == Some(s) && self.in_package_object(p) => inst,
            _ => module,
        };
        Ok(self.without_repeated_annot(wide))
    }

    /// `t` without a top-level `@Repeated` annotation (dotty's `dropRepeatedAnnot`).
    fn without_repeated_annot(&mut self, t: TypeId) -> TypeId {
        match self.u.annotated(t) {
            Some((inner, annot)) if self.is_repeated_annot(annot) => inner,
            _ => t,
        }
    }

    /// `t` with its singletons widened if the result conforms to `bound` (dotty's `widenSingle`).
    fn widen_single(&mut self, t: TypeId, bound: TypeId, skip_soft_unions: bool) -> TypeId {
        let widened = self.widen_singletons(t, skip_soft_unions);
        if widened != t && self.conforms(widened, bound) { widened } else { t }
    }

    /// `t`, dealiased, with its singletons widened, also in intersections and, unless
    /// `skip_soft_unions`, soft unions, whose parts then join if one contains the others; `t` if
    /// nothing widens (dotty's `widenSingletons`).
    pub fn widen_singletons(&mut self, t: TypeId, skip_soft_unions: bool) -> TypeId {
        let d = self.u.dealias(t);
        match self.u.ty(d).clone() {
            Type::Or(a, b, true) if !skip_soft_unions => {
                let (wa, wb) = (self.widen_singletons(a, false), self.widen_singletons(b, false));
                if (wa, wb) == (a, b) { t } else { self.u.lub(wa, wb) }
            }
            Type::And(a, b) => {
                let (wa, wb) =
                    (self.widen_singletons(a, skip_soft_unions), self.widen_singletons(b, skip_soft_unions));
                if (wa, wb) == (a, b) { t } else { self.u.glb(wa, wb) }
            }
            _ if self.is_singleton(d) => {
                let widened = self.types().widened(d);
                if self.is_singleton_bounded(widened) { t } else { widened }
            }
            _ => t,
        }
    }

    /// Whether `t` is an abstract type or type parameter with a singleton upper bound.
    fn is_singleton_bounded(&mut self, t: TypeId) -> bool {
        let Some(s) = self.u.sym_of(t) else { return false };
        if !matches!(self.u.symbol(s).kind, Kind::TypeParam | Kind::Type) {
            return false;
        }
        self.complete(s);
        let info = self.u.info(s);
        match self.u.bounds(info) {
            Some((_, hi)) => self.is_singleton(hi),
            None => false,
        }
    }

    /// Whether `t` is a singleton, `Singleton`, or an abstract type or intersection below one
    /// (dotty's `isSingletonBounded`), looking `depth` bounds deep at most 16.
    pub fn is_below_singleton(&mut self, t: TypeId, depth: u32) -> bool {
        if self.is_singleton(t) {
            return true;
        }
        if depth > 16 {
            return false;
        }
        match *self.u.ty(t) {
            Type::And(a, b) => self.is_below_singleton(a, depth + 1) || self.is_below_singleton(b, depth + 1),
            Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::TypeParam | Kind::Type) => {
                self.complete(s);
                let info = self.u.info(s);
                match *self.u.ty(info) {
                    Type::Bounds(_, hi) | Type::Alias(hi) => self.is_below_singleton(hi, depth + 1),
                    _ => false,
                }
            }
            Type::Var(v) => {
                let next = self.constraint.instance(v).unwrap_or(self.constraint.bounds(v).1);
                self.is_below_singleton(next, depth + 1)
            }
            Type::Sym(_, c) => self.u.full_name(c) == "scala.Singleton",
            _ => false,
        }
    }

    /// Whether `prefix` is the `this` of a package object.
    fn in_package_object(&mut self, prefix: TypeId) -> bool {
        let Some(c) = self.u.this_of(prefix) else { return false };
        self.u.class_symbol(c).is_some_and(|c| self.is_named_package_object(c))
    }
}

impl TypeOps<'_> {
    /// Replaces a reference to a module class by a reference to its object; `t` otherwise.
    pub fn module_ref(&mut self, t: TypeId) -> TypeId {
        let Some((prefix, c)) = self.u.sym_ref(t) else { return t };
        if !self.u.is_module_class(c) {
            return t;
        }
        match self.u.source_module(c) {
            Some(m) => self.u.intern_type(Type::Sym(prefix, m)),
            None => t,
        }
    }
}
