// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! The variables approximating a dependent method's parameters in its expected result,
//! constrained by the arguments (dotty's `constrainIfDependentParamRef`).

use super::Typer;
use super::stability::is_stable_path;
use tasty::{SymbolId, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Constrains each pending variable of `approx_vars`, which approximated a parameter in the
    /// expected result of an application (`result_type_approx`), from below by its argument as a
    /// singleton: `singletons`, what the result substituted, else the argument's type `args` if
    /// stable, else a skolem of it. Refuses an argument local to a variable whose bounds refer to
    /// the approximation, which dotty avoids by a variable above it (`legalVar`).
    pub fn constrain_approx_vars(
        &mut self,
        approx_vars: &[TypeId],
        (singletons, args): (&[TypeId], &[TypeId]),
    ) -> Result<(), &'static str> {
        for (&var, (&s, &arg)) in approx_vars.iter().zip(singletons.iter().zip(args)) {
            let Some(v) = self.u.var(var).filter(|&v| self.constraint.is_pending(v)) else { continue };
            let singleton = match s != arg || is_stable_path(self.u, arg) {
                true => s,
                false => {
                    let widened = self.u.widen(arg);
                    self.u.fresh_skolem(widened)
                }
            };
            if self.escapes_through(v, singleton) {
                return Err("local type escaping through a type variable");
            }
            self.u.is_subtype_in(singleton, var, &mut self.constraint);
        }
        Ok(())
    }

    /// Whether `singleton` refers to a value defined after a pending variable other than `v`
    /// whose bounds refer to `v`.
    fn escapes_through(&mut self, v: VarId, singleton: TypeId) -> bool {
        let mut locals: Vec<SymbolId> = Vec::new();
        self.u.map_type(singleton, &mut |u, part| match *u.ty(part) {
            Type::Sym(_, s) if u.symbol(s).kind.is_term() => {
                locals.push(s);
                None
            }
            Type::Skolem(..) => Some(part),
            _ => None,
        });
        let others: Vec<VarId> = self.constraint.pending_from(0).filter(|&w| w != v).collect();
        others.into_iter().any(|w| {
            let (lo, hi) = self.constraint.bounds(w);
            let refers = [lo, hi].into_iter().any(|b| {
                let b = self.zonk(b);
                let mut found = false;
                self.u.map_type(b, &mut |u, part| {
                    found |= u.var(part) == Some(v);
                    None
                });
                found
            });
            refers && locals.iter().any(|&s| !self.constraint.made_after(w, s))
        })
    }
}
