// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! GADT bounds: the bounds a case's pattern gives the type parameters of enclosing methods and
//! classes, in force while the case is typed.

use super::Typer;
use crate::ctx::Ctx;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

/// The type parameters narrowed by the patterns of the cases being typed.
#[derive(Default)]
pub struct Gadt {
    /// Each narrowed symbol with its info before, innermost last.
    saved: Vec<(SymbolId, TypeId)>,
    /// How many cases are being typed.
    cases: u32,
}

impl Typer<'_> {
    /// `f` as the typing of a case: the bounds its pattern narrows restored after.
    pub fn in_gadt_case<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let mark = self.gadt.saved.len();
        self.gadt.cases += 1;
        let result = f(self);
        self.gadt.cases -= 1;
        let narrowed: Vec<(SymbolId, TypeId)> = self.gadt.saved.drain(mark..).rev().collect();
        for (s, info) in narrowed {
            self.u.set_info(s, info);
        }
        result
    }

    /// The type parameters in `t` a pattern may narrow: those of the methods enclosing `ctx`'s
    /// owner and of the classes enclosing one of them (dotty's `addToConstraint` in `typedDefDef`).
    fn gadt_symbols(&mut self, t: TypeId, ctx: Ctx) -> Vec<SymbolId> {
        if self.gadt.cases == 0 {
            return Vec::new();
        }
        let mut owners = Vec::new();
        let (mut owner, mut in_method) = (Some(ctx.owner), false);
        while let Some(o) = owner {
            match self.u.symbol(o).kind {
                Kind::Def => {
                    in_method = true;
                    owners.push(o);
                }
                Kind::Class if in_method => owners.push(o),
                _ => {}
            }
            owner = self.u.symbol(o).owner;
        }
        let mut syms = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(None, s) = *u.types().get(part)
                && u.symbol(s).kind == Kind::TypeParam
                && u.symbol(s).owner.is_some_and(|o| owners.contains(&o))
                && !syms.contains(&s)
            {
                syms.push(s);
            }
            None
        });
        syms
    }

    /// Whether pattern type `pat` conforms to `sel` with `sel`'s narrowable type parameters
    /// as type variables of their bounds, constraining them; those parameters and variables.
    pub fn gadt_vars(
        &mut self,
        pat: TypeId,
        sel: TypeId,
        ctx: Ctx,
    ) -> Result<Option<Vec<(SymbolId, TypeId)>>, &'static str> {
        let syms = self.gadt_symbols(sel, ctx);
        if syms.is_empty() {
            return Ok(None);
        }
        let vars: Vec<TypeId> = syms.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let subst = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Sym(None, s) => syms.iter().position(|&b| b == s).map(|i| vars[i]),
                _ => None,
            })
        };
        for (&s, &v) in syms.iter().zip(&vars) {
            let info = self.u.info(s);
            if let Type::Bounds(lo, hi) = *self.u.types().get(info) {
                let (lo, hi) = (subst(self.u, lo), subst(self.u, hi));
                self.constraint.declare(self.u, v, lo, hi);
            }
        }
        let sel = subst(self.u, sel);
        let ok = self.constrain_pattern(pat, sel)?;
        Ok(ok.then(|| syms.into_iter().zip(vars).collect()))
    }

    /// Gives each symbol of `vars` its variable's bounds, which must be known, while the case is
    /// typed; the variables instantiated to their upper bounds.
    pub fn narrow_gadt(&mut self, vars: &[(SymbolId, TypeId)]) -> Result<(), &'static str> {
        let mut bounds = Vec::new();
        for &(s, v) in vars {
            let Type::Var(id) = *self.u.types().get(v) else { continue };
            let (lo, hi) = self.constraint.instance(id).map_or_else(|| self.constraint.bounds(id), |t| (t, t));
            let (lo, hi) = (self.zonk(lo), self.zonk(hi));
            if [lo, hi].iter().any(|&t| self.has_type_var(t)) {
                return Err("GADT bound of an undetermined type");
            }
            if self.constraint.instance(id).is_none() {
                self.constraint.instantiate(id, hi);
            }
            bounds.push((s, lo, hi));
        }
        for (s, lo, hi) in bounds {
            let info = self.u.intern_type(Type::Bounds(lo, hi));
            let old = self.u.info(s);
            if info != old {
                self.gadt.saved.push((s, old));
                self.u.set_info(s, info);
            }
        }
        Ok(())
    }

    /// The bounds of the GADT variables `vars`, zonked.
    pub fn gadt_var_bounds(&mut self, vars: &[(SymbolId, TypeId)]) -> Vec<TypeId> {
        let mut bounds = Vec::new();
        for &(_, v) in vars {
            let Type::Var(id) = *self.u.types().get(v) else { continue };
            let (lo, hi) = self.constraint.bounds(id);
            bounds.extend([self.zonk(lo), self.zonk(hi)]);
        }
        bounds
    }

    /// Whether `t` refers to type `part`.
    pub fn refers_to_type(&mut self, t: TypeId, part: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |_, p| {
            found |= p == part;
            None
        });
        found
    }

    /// Whether `t` refers to a type variable.
    fn has_type_var(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, p| {
            found |= matches!(u.types().get(p), Type::Var(_));
            None
        });
        found
    }

    /// Whether `a` conforms to `b` without the GADT bounds, frozen.
    pub fn conforms_without_gadt(&mut self, a: TypeId, b: TypeId) -> bool {
        let (a, b) = (self.zonk(a), self.zonk(b));
        let saved = self.gadt.saved.clone();
        let current: Vec<(SymbolId, TypeId)> = saved.iter().map(|&(s, _)| (s, self.u.info(s))).collect();
        for &(s, info) in saved.iter().rev() {
            self.u.set_info(s, info);
        }
        let ok = self.u.is_subtype(a, b);
        for (s, info) in current {
            self.u.set_info(s, info);
        }
        ok
    }

    /// Whether GADT bounds are in force.
    pub fn gadt_narrowing(&self) -> bool {
        !self.gadt.saved.is_empty()
    }

    /// The types GADT bounds narrow.
    pub fn gadt_narrowed(&self) -> Vec<SymbolId> {
        self.gadt.saved.iter().map(|&(s, _)| s).collect()
    }
}
