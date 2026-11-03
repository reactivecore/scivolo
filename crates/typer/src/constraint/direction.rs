// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The declared bounds of variables, the direction they are instantiated in by them (dotty's
//! `instDirection`) and the kinds they admit.

use super::Constraint;
use tasty::{ANY, Kind, NOTHING, Type, TypeId, TypeVars, Universe, VarId};

impl Constraint {
    /// Records `lo` and `hi` as the declared bounds of `t` if it is a variable.
    pub fn declare(&mut self, u: &Universe, t: TypeId, lo: TypeId, hi: TypeId) {
        if let Type::Var(v) = *u.types().get(t) {
            self.declared.resize(self.vars.len(), (NOTHING, ANY));
            self.declared[v.0 as usize] = (lo, hi);
            self.declared_vars.insert(v.0);
        }
    }

    /// 1 (maximize) if `v`'s bounds are narrower than its declared ones only from above, -1
    /// (minimize) if only from below, else 0.
    pub fn direction(&mut self, u: &mut Universe, v: VarId) -> i8 {
        let (lo, hi) = self.bounds(v);
        let (declared_lo, declared_hi) = self.declared.get(v.0 as usize).copied().unwrap_or((NOTHING, ANY));
        let below = !u.is_subtype_frozen_in(lo, declared_lo, self);
        let above = !u.is_subtype_frozen_in(declared_hi, hi, self);
        above as i8 - below as i8
    }

    /// Whether bound `t` may be added to `v`: not a type constructor if `v` is declared with a
    /// first-order upper bound other than `AnyKind` (dotty's `kindCompatible`).
    pub fn kind_compatible(&self, u: &mut Universe, v: VarId, t: TypeId) -> bool {
        if !self.declared_vars.contains(&v.0) {
            return true;
        }
        let hi = u.dealias(self.declared[v.0 as usize].1);
        let any_kind = u.class_symbol(hi) == Some(u.builtins().any_kind);
        any_kind || matches!(u.types().get(hi), Type::Lambda { .. }) || !is_constructor(u, t)
    }
}

/// Whether `t` dealiased is a type lambda or an unapplied class or type parameter that takes
/// parameters.
fn is_constructor(u: &mut Universe, t: TypeId) -> bool {
    let t = u.dealias(t);
    match *u.types().get(t) {
        Type::Lambda { .. } => true,
        Type::Sym(_, s) if u.symbol(s).kind == Kind::Class => !u.type_params(s).is_empty(),
        Type::Sym(_, s) if u.symbol(s).kind == Kind::TypeParam => {
            let info = u.info(s);
            matches!(*u.types().get(info), Type::Bounds(_, hi) if matches!(u.types().get(hi), Type::Lambda { .. }))
        }
        _ => false,
    }
}
