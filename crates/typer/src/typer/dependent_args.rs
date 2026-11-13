// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! The variables approximating a dependent method's parameters in its expected result,
//! constrained by the arguments (dotty's `constrainIfDependentParamRef`), and the members of
//! instantiated variables.

use super::Typer;
use super::infer::zonked;
use super::stability::is_stable_path;
use tasty::{SymbolId, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Constrains each pending variable of `approx_vars`, which approximated a parameter in the
    /// expected result of an application (`result_type_approx`), from below by its argument as a
    /// singleton: `singletons`, what the result substituted, else the argument's type `args` if
    /// stable, else a skolem of it. Refuses an argument local to a variable whose bounds refer to
    /// the approximation, directly or through other variables, which dotty avoids by a variable
    /// above it (`legalVar`).
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
    /// whose bounds refer to `v`, directly or through other pending variables.
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

        let pending: Vec<VarId> = self.constraint.pending_from(0).collect();
        let mut reaching = vec![v];
        let mut grown = true;
        while grown {
            grown = false;
            for &w in &pending {
                if !reaching.contains(&w) && self.bounds_refer(w, &reaching) {
                    reaching.push(w);
                    grown = true;
                }
            }
        }
        reaching[1..].iter().any(|&w| locals.iter().any(|&s| !self.constraint.made_after(w, s)))
    }

    /// Replaces in `t` each type member of an instantiated variable that a refinement of the
    /// variable's instance aliases by the alias (dotty's `simplify` of a selection on an
    /// instantiated `TypeVar`, `derivedSelect` with `lookupRefined`).
    pub fn with_refined_var_members(&mut self, t: TypeId) -> TypeId {
        let c = &self.constraint;
        self.u.map_type(t, &mut |u, part| {
            let Some((Some(pre), s)) = u.sym_ref(part) else { return None };
            let inst = zonked(u, c, c.instance(u.var(pre)?)?, false);
            if !matches!(u.ty(inst), Type::Skolem(..) | Type::Refined(..)) {
                return None;
            }
            u.refinement_info(inst, s).and_then(|i| u.type_alias(i))
        })
    }

    /// Whether a bound of variable `w` refers to one of `vars`.
    fn bounds_refer(&mut self, w: VarId, vars: &[VarId]) -> bool {
        let (lo, hi) = self.constraint.bounds(w);
        [lo, hi].into_iter().any(|b| {
            let b = self.zonk(b);
            let mut found = false;
            self.u.map_type(b, &mut |u, part| {
                found |= u.var(part).is_some_and(|x| vars.contains(&x));
                None
            });
            found
        })
    }
}
