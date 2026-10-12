//! Expected types of implicit searches whose type variables the search cannot settle as dotty
//! does.

use super::super::Typer;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether `t` has an uninstantiated type variable without bounds.
    pub fn has_unbounded_var(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        let mut vars = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part) {
                vars.push(v);
            }
            None
        });
        vars.into_iter().any(|v| self.constraint.bounds(v) == (tasty::NOTHING, tasty::ANY))
    }

    /// Whether `t`, its aliases expanded, has a union with an uninstantiated type variable in it.
    pub fn has_open_union(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Or(..) = *u.types().get(part) {
                u.map_type(part, &mut |u, p| {
                    found |= matches!(u.types().get(p), Type::Var(_));
                    None
                });
            }
            None
        });
        found
    }
}
