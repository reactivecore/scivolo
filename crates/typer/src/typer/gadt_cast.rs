// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! The cast to an expected type a tree conforms to only by GADT bounds (dotty's
//! `insertGadtCast`, `approximateGADT`).

use super::Typer;
use super::gadt::plain_type_ref;
use super::gadt_cases::Gadt;
use super::gadt_full_bounds::is_top;
use super::stability::is_stable_path;
use super::type_ops::TypeOps;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId, Universe};

pub const GADT_HEALED_VIEW: &str = "implicit conversion of a GADT-approximated qualifier";

/// Whether `t` is stable (dotty's `isStable`), `annot` marking a type `@uncheckedStable`.
fn is_stable(u: &mut Universe, t: TypeId, annot: TypeId) -> bool {
    match *u.ty(t) {
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
    match *u.ty(t) {
        Type::Sym(_, s) => u.is_class(s),
        Type::TypeRef(..) => u.resolve_ref(t).is_some_and(|s| u.is_class(s)),
        Type::Applied(tycon, _) => is_concrete(u, tycon),
        Type::Lambda { result, .. } => is_concrete(u, result),
        Type::And(a, b) | Type::Or(a, b, _) => is_concrete(u, a) && is_concrete(u, b),
        Type::Annotated(a, _) => is_concrete(u, a),
        _ => false,
    }
}

impl Typer<'_> {
    /// Casts `tree` to `pt` with `$asInstanceOf$`; a singleton to itself, its widened type
    /// approximated without the GADT bounds, and `pt` unless that approximation conforms to it.
    pub fn gadt_cast(&mut self, tree: TNodeId, pt: TypeId, ctx: Ctx) -> TNodeId {
        let tpe = self.tpd.tpe(tree);
        if !self.is_singleton_tree(tree) {
            return self.type_cast(tree, pt);
        }
        if self.gadt.inline {
            let span = self.tpd.span(tree);
            return self.unsupported_tree(span, "GADT cast of a singleton in an inline match case");
        }
        let underlying = self.u.annotated(tpe).map_or(tpe, |(t, _)| t);
        let widened = self.u.widen(underlying);
        let approx = self.gadt.approximate(self.u, widened, 1, ctx);
        let conj = self.u.intern_type(Type::And(tpe, approx));
        let conj = match self.conforms_without_gadt(approx, pt) {
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

    /// Whether the type of `tree`, an annotation dropped, is a singleton.
    fn is_singleton_tree(&mut self, tree: TNodeId) -> bool {
        let tpe = self.tpd.tpe(tree);
        let underlying = self.u.annotated(tpe).map_or(tpe, |(t, _)| t);
        self.is_singleton(underlying)
    }

    /// Tells whether `a` conforms to `b` without the GADT bounds, constraining type variables.
    fn conforms_without_gadt(&mut self, a: TypeId, b: TypeId) -> bool {
        let ops = &mut TypeOps { u: self.u, constraint: &mut self.constraint };
        self.gadt.conforms_without(ops, a, b)
    }

    /// Casts `tree` to `pt` if it conforms to it only by the bounds of the cases being typed, as
    /// a case body `i: Int` returned for a `T` known to be `Int` (dotty's `gadtAdaptBranch`);
    /// type variables constrained by the comparison without them first.
    pub fn gadt_adapted(&mut self, tree: TNodeId, pt: TypeId, ctx: Ctx) -> TNodeId {
        let tpe = self.tpd.tpe(tree);
        match self.gadt.narrowing() && !self.conforms_without_gadt(tpe, pt) && self.conforms(tpe, pt) {
            true => self.gadt_cast(tree, pt, ctx),
            false => tree,
        }
    }

    /// Applies `gadt_adapted` to a block's expression, as dotty adapts it while typing the
    /// block; to a closure as a whole.
    pub fn gadt_adapted_in(&mut self, tree: TNodeId, pt: TypeId, ctx: Ctx) -> TNodeId {
        let T::Block { stats, expr } = *self.tpd.tree(tree) else { return self.gadt_adapted(tree, pt, ctx) };
        if matches!(self.tpd.tree(expr), T::Closure { .. }) {
            return self.gadt_adapted(tree, pt, ctx);
        }
        match self.gadt_adapted_in(expr, pt, ctx) {
            adapted if adapted == expr => self.gadt_adapted(tree, pt, ctx),
            _ if self.is_singleton_tree(expr) => {
                let span = self.tpd.span(tree);
                self.unsupported_tree(span, "GADT cast of a singleton as a block's expression")
            }
            adapted => {
                let span = self.tpd.span(tree);
                self.build().block(stats, adapted, span)
            }
        }
    }

    /// Casts qualifier `qual` to its widened type approximated by the GADT bounds if that differs,
    /// so that `b.value` selects on `IntBox` where `b: Box[T]` and `T = Int` (dotty's `tryGadt` in
    /// `typedSelect`).
    pub fn gadt_healed(&mut self, qual: TNodeId, ctx: Ctx) -> Option<TNodeId> {
        if !self.gadt.narrowing() {
            return None;
        }
        let widened = self.tpd.tpe(qual);
        let widened = self.u.widen(widened);
        let approx = self.gadt.approximate(self.u, widened, 1, ctx);
        (approx != widened).then(|| self.type_cast(qual, approx))
    }
}

impl Gadt {
    /// Replaces in `t` each type a case may narrow by its upper bound where it occurs
    /// covariantly, by its lower bound where contravariantly, not inside the bounds, as `Box[T]`
    /// becomes `Box[Int]` for `T = Int`; a covariant application of one without a proper upper
    /// bound or made one with another by `Any`.
    fn approximate(&self, u: &mut Universe, t: TypeId, variance: i8, ctx: Ctx) -> TypeId {
        let t = u.dealias(t);
        if let Some(s) = plain_type_ref(u, t)
            && variance != 0
            && self.is_symbol(u, s, ctx)
        {
            return self.bound(u, s, variance);
        }
        match *u.ty(t) {
            Type::Applied(tycon, args) => {
                let args = u.types_of(args).to_vec();
                let variances = u.tycon_variances(tycon, args.len());
                let args: Vec<_> = args
                    .into_iter()
                    .zip(variances)
                    .map(|(a, v)| self.approximate(u, a, variance * v, ctx))
                    .collect();
                let tycon = match plain_type_ref(u, tycon) {
                    Some(s) if variance != 0 && self.is_symbol(u, s, ctx) => {
                        let bound = self.bound(u, s, variance);
                        let made_one = u.gadt_bounds(s).is_some_and(|(lo, hi)| {
                            lo == hi && plain_type_ref(u, hi).is_some_and(|r| r != s)
                        });
                        if variance > 0 && (is_top(u, bound) || made_one) {
                            return tasty::ANY;
                        }
                        let simple = u.lambda(bound).is_none();
                        if simple && bound != tasty::ANY && bound != tasty::NOTHING { bound } else { tycon }
                    }
                    _ => tycon,
                };
                u.intern_applied(tycon, &args)
            }
            Type::And(a, b) => {
                let (a, b) = (self.approximate(u, a, variance, ctx), self.approximate(u, b, variance, ctx));
                u.intern_type(Type::And(a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (self.approximate(u, a, variance, ctx), self.approximate(u, b, variance, ctx));
                u.intern_type(Type::Or(a, b, soft))
            }
            _ => t,
        }
    }
}
