// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Expected types with their open type variables approximated by wildcards (dotty's
//! `wildApprox`), as transparent inline methods constrain their results.

use super::Typer;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The type that constrains the result of method `meth`: `pt`, approximated by wildcards if
    /// `meth` is a transparent inline method (dotty's `constrainResult(meth, mt, pt)`).
    pub fn result_pt(&mut self, meth: Option<SymbolId>, pt: TypeId) -> TypeId {
        match meth {
            Some(m) if self.is_transparent_inline(m) => self.wild_approx(pt),
            _ => pt,
        }
    }

    /// `t` with its uninstantiated type variables replaced by wildcards of their bounds.
    pub fn wild_approx(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        let constraint = &self.constraint;
        self.u.map_type(t, &mut |u, part| {
            let Type::Var(v) = *u.types().get(part) else { return None };
            let (lo, hi) = constraint.bounds(v);
            let mut open = false;
            for b in [lo, hi] {
                u.map_type(b, &mut |u, x| {
                    open |= matches!(u.types().get(x), Type::Var(_));
                    None
                });
            }
            let bounds = (!open).then(|| u.intern_type(Type::Bounds(lo, hi)));
            Some(u.intern_type(Type::Wildcard(bounds)))
        })
    }
}
