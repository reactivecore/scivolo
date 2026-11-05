// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! The cast to an expected type a tree conforms to only by GADT bounds (dotty's
//! `insertGadtCast`, `approximateGADT`).

use super::Typer;
use super::stability::is_stable_path;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId, Universe};

/// Whether `t` is stable (dotty's `isStable`), `annot` marking a type `@uncheckedStable`.
fn is_stable(u: &mut Universe, t: TypeId, annot: TypeId) -> bool {
    match *u.types().get(t) {
        Type::Annotated(p, a) => a == annot || is_stable(u, p, annot),
        Type::And(a, b) => {
            is_stable(u, a, annot) && is_concrete(u, b) || is_stable(u, b, annot) && is_concrete(u, a)
        }
        _ => is_stable_path(u, t),
    }
}

/// Whether `t` is a class type or made of them (`isConcrete` of dotty's `realizability`).
fn is_concrete(u: &mut Universe, t: TypeId) -> bool {
    let t = u.dealias(t);
    match *u.types().get(t) {
        Type::Sym(_, s) => u.is_class(s),
        Type::Applied(tycon, _) => is_concrete(u, tycon),
        Type::And(a, b) | Type::Or(a, b, _) => is_concrete(u, a) && is_concrete(u, b),
        Type::Annotated(a, _) => is_concrete(u, a),
        _ => false,
    }
}

impl Typer<'_> {
    /// `tree.$asInstanceOf$[target]`: `pt`, or for a singleton the singleton and its widened type
    /// approximated without the GADT bounds, and `pt` unless that approximation conforms to it.
    pub fn gadt_cast(&mut self, tree: TNodeId, pt: TypeId, ctx: Ctx) -> TNodeId {
        let tpe = self.tpd.tpe(tree);
        let underlying = match *self.u.types().get(tpe) {
            Type::Annotated(t, _) => t,
            _ => tpe,
        };
        if !self.is_singleton(underlying) {
            return self.type_cast(tree, pt);
        }
        let widened = self.u.widen(underlying);
        let approx = self.approximate_gadt(widened, 1, ctx);
        let conj = self.u.intern_type(Type::And(tpe, approx));
        let conj = match self.conforms(approx, pt) && self.conforms_without_gadt(approx, pt) {
            true => conj,
            false => self.u.intern_type(Type::And(conj, pt)),
        };
        let annot = self.u.class_ref("scala.annotation.unchecked.uncheckedStable");
        let target = match is_stable(self.u, tpe, annot) && !is_stable(self.u, conj, annot) {
            true => self.u.intern_type(Type::Annotated(conj, annot)),
            false => conj,
        };
        self.type_cast(tree, target)
    }

    /// `tree` cast to `pt` if it conforms to it only by GADT bounds (dotty's `gadtAdaptBranch`).
    pub fn gadt_adapted(&mut self, tree: TNodeId, pt: TypeId, ctx: Ctx) -> TNodeId {
        let tpe = self.tpd.tpe(tree);
        match self.gadt_narrowing() && !self.conforms_without_gadt(tpe, pt) && self.conforms(tpe, pt) {
            true => self.gadt_cast(tree, pt, ctx),
            false => tree,
        }
    }

    /// `t` with each GADT-constrained type replaced by its upper bound where it occurs
    /// covariantly, by its lower bound where contravariantly, not inside the bounds.
    fn approximate_gadt(&mut self, t: TypeId, variance: i8, ctx: Ctx) -> TypeId {
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Sym(None, s) if variance != 0 && self.is_gadt_symbol(s, ctx) => {
                self.gadt_bound(s, variance)
            }
            Type::Applied(tycon, args) => {
                let variances = self.u.tycon_variances(tycon, args.len());
                let args = args
                    .iter()
                    .zip(variances)
                    .map(|(&a, v)| self.approximate_gadt(a, variance * v, ctx))
                    .collect();
                self.u.intern_type(Type::Applied(tycon, args))
            }
            Type::And(a, b) => {
                let (a, b) =
                    (self.approximate_gadt(a, variance, ctx), self.approximate_gadt(b, variance, ctx));
                self.u.intern_type(Type::And(a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) =
                    (self.approximate_gadt(a, variance, ctx), self.approximate_gadt(b, variance, ctx));
                self.u.intern_type(Type::Or(a, b, soft))
            }
            _ => t,
        }
    }

    /// `s`'s upper bound for `variance` 1, its lower bound for -1; a top upper bound a GADT
    /// bound of a type whose lower bound is `s`.
    fn gadt_bound(&mut self, s: SymbolId, variance: i8) -> TypeId {
        let own = self.u.intern_type(Type::Sym(None, s));
        let Some((lo, hi)) = self.current_bounds(s) else { return own };
        if variance < 0 {
            return lo;
        }
        if hi != tasty::ANY {
            return hi;
        }
        let above =
            self.gadt_narrowed().into_iter().find(|&n| self.u.gadt_bounds(n).is_some_and(|(l, _)| l == own));
        above.map_or(hi, |n| self.u.intern_type(Type::Sym(None, n)))
    }
}
