// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! Instantiating the type variables of a type that does not fit as it is (dotty's
//! `canDefineFurther` with `ForceDegree.failBottom`).

use super::Typer;
use tasty::{NOTHING, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Whether `tpe` conforms to `pt` once the type variables in `tpe` are instantiated; the
    /// constraint is kept only if it does.
    pub fn defined_further(&mut self, tpe: TypeId, pt: TypeId) -> bool {
        let mark = self.constraint.mark();
        let ok = self.defined_fail_bottom(tpe).is_some_and(|t| self.conforms(t, pt));
        if !ok {
            self.constraint.rollback(mark);
        }
        ok
    }

    /// `t` with its type variables instantiated, `None` with the constraint as it was if it has
    /// none or one is instantiated to `Nothing`.
    pub fn defined_fail_bottom(&mut self, t: TypeId) -> Option<TypeId> {
        let zonked = self.zonk(t);
        let mut open: Vec<VarId> = Vec::new();
        self.u.map_type(zonked, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part)
                && !open.contains(&v)
            {
                open.push(v);
            }
            None
        });
        if open.is_empty() {
            return None;
        }
        let mark = self.constraint.mark();
        let defined = self.fully_defined_as(t, true).ok();
        let nothing = self.u.builtins().nothing;
        let bottom = open.iter().any(|&v| {
            let instance = self.constraint.instance(v).map(|i| self.zonk(i));
            instance.is_some_and(|i| i == NOTHING || self.u.class_symbol(i) == Some(nothing))
        });
        if defined.is_none() || bottom {
            self.constraint.rollback(mark);
            return None;
        }
        defined
    }
}
