// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! The cast to an expected type a tree conforms to only by GADT bounds (dotty's
//! `insertGadtCast`, `approximateGADT`).

use super::Typer;
use super::stability::is_stable_path;
use crate::tpd::TNodeId;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `tree.$asInstanceOf$[target]`: `pt`, or for a singleton the singleton and its widened type
    /// approximated without the GADT bounds, and `pt` unless that approximation conforms to it.
    pub fn gadt_cast(&mut self, tree: TNodeId, pt: TypeId) -> TNodeId {
        let tpe = self.tpd.tpe(tree);
        if !self.is_singleton(tpe) {
            return self.type_cast(tree, pt);
        }
        let widened = self.u.widen(tpe);
        let approx = self.approximate_gadt(widened, true);
        let conj = self.u.intern_type(Type::And(tpe, approx));
        let conj = match self.conforms(approx, pt) && self.conforms_without_gadt(approx, pt) {
            true => conj,
            false => self.u.intern_type(Type::And(conj, pt)),
        };
        let target = match is_stable_path(self.u, tpe) && !is_stable_path(self.u, conj) {
            true => {
                let annot = self.u.class_ref("scala.annotation.unchecked.uncheckedStable");
                self.u.intern_type(Type::Annotated(conj, annot))
            }
            false => conj,
        };
        self.type_cast(tree, target)
    }

    /// `t` with each GADT-narrowed type replaced by its upper bound where it occurs covariantly,
    /// by its lower bound where contravariantly.
    fn approximate_gadt(&mut self, t: TypeId, covariant: bool) -> TypeId {
        let narrowed = self.gadt_narrowed();
        self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) if narrowed.contains(&s) => {
                let info = u.info(s);
                match *u.types().get(info) {
                    Type::Bounds(lo, hi) => Some(if covariant { hi } else { lo }),
                    _ => None,
                }
            }
            _ => None,
        })
    }
}
