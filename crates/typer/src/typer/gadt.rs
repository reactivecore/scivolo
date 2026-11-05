// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! GADT bounds: the bounds a case's pattern gives the type parameters of enclosing methods and
//! classes, in force while the case is typed.

use super::Typer;
use crate::ctx::Ctx;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

/// The type parameters narrowed by the patterns of the cases being typed.
#[derive(Default)]
pub struct Gadt {
    /// Each narrowed symbol with its GADT bounds before, innermost last.
    saved: Vec<(SymbolId, Option<(TypeId, TypeId)>)>,
    /// The pattern-bound types of the cases being typed.
    bound: Vec<SymbolId>,
    /// How many cases are being typed.
    cases: u32,
}

impl Typer<'_> {
    /// `f` as the typing of a case: the bounds its pattern narrows restored after.
    pub fn in_gadt_case<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let (mark, bound) = (self.gadt.saved.len(), self.gadt.bound.len());
        self.gadt.cases += 1;
        let result = f(self);
        self.gadt.cases -= 1;
        self.gadt.bound.truncate(bound);
        let narrowed: Vec<_> = self.gadt.saved.drain(mark..).rev().collect();
        for (s, bounds) in narrowed {
            self.u.set_gadt_bounds(s, bounds);
        }
        result
    }

    /// Adds pattern-bound types `syms` to the GADT-constrained types of the case being typed.
    pub fn gadt_bind(&mut self, syms: &[SymbolId]) {
        if self.gadt.cases > 0 {
            self.gadt.bound.extend(syms);
        }
    }

    /// Narrows `s` to `bounds` while the case is typed; a pattern-bound type gets them as its
    /// info (dotty's `indexPattern`).
    pub fn set_gadt_bounds(&mut self, s: SymbolId, (lo, hi): (TypeId, TypeId)) {
        if self.gadt.bound.contains(&s) {
            let info = self.u.intern_type(Type::Bounds(lo, hi));
            self.u.set_info(s, info);
        } else {
            let old = self.u.set_gadt_bounds(s, Some((lo, hi)));
            self.gadt.saved.push((s, old));
        }
    }

    /// Whether `s` is a GADT-constrained type: a type parameter of a method enclosing `ctx`'s
    /// owner or of a class enclosing one (dotty's `addToConstraint` in `typedDefDef`), or a
    /// pattern-bound type of a case being typed.
    pub fn is_gadt_symbol(&mut self, s: SymbolId, ctx: Ctx) -> bool {
        if self.gadt.bound.contains(&s) {
            return true;
        }
        let Some(of) = self.u.symbol(s).owner.filter(|_| self.u.symbol(s).kind == Kind::TypeParam) else {
            return false;
        };
        let (mut owner, mut in_method) = (Some(ctx.owner), false);
        while let Some(o) = owner {
            match self.u.symbol(o).kind {
                Kind::Def => in_method = true,
                Kind::Class if in_method => {}
                _ => {
                    owner = self.u.symbol(o).owner;
                    continue;
                }
            }
            if o == of {
                return true;
            }
            owner = self.u.symbol(o).owner;
        }
        false
    }

    /// The type parameters in `t` a pattern may narrow; none if `t` has a part outside the
    /// subset: a refinement, a singleton, a higher-kinded parameter or an abstract type
    /// constructor applied.
    pub fn gadt_symbols(&mut self, t: TypeId, ctx: Ctx) -> Vec<SymbolId> {
        if self.gadt.cases == 0 {
            return Vec::new();
        }
        let mut params = Vec::new();
        if !self.first_order_params(t, &mut params) {
            return Vec::new();
        }
        params.into_iter().filter(|&s| self.is_gadt_symbol(s, ctx)).collect()
    }

    /// Adds the type parameters `t` refers to outside prefixes to `params`; false if `t` has a
    /// refinement, a singleton, a higher-kinded type parameter or an abstract type constructor
    /// applied.
    fn first_order_params(&mut self, t: TypeId, params: &mut Vec<SymbolId>) -> bool {
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::TypeParam || self.gadt.bound.contains(&s) => {
                let info = self.u.info(s);
                let Type::Bounds(_, hi) = *self.u.types().get(info) else { return false };
                if matches!(self.u.types().get(hi), Type::Lambda { .. }) {
                    return false;
                }
                if !params.contains(&s) {
                    params.push(s);
                }
                true
            }
            Type::Sym(_, s) => !self.u.symbol(s).kind.is_term(),
            Type::Applied(tycon, args) => {
                self.u.class_symbol(tycon).is_some_and(|c| self.u.is_class(c))
                    && args.iter().all(|&a| self.first_order_params(a, params))
            }
            Type::And(a, b) | Type::Or(a, b, _) | Type::Bounds(a, b) => {
                self.first_order_params(a, params) && self.first_order_params(b, params)
            }
            Type::Annotated(a, _) | Type::Alias(a) => self.first_order_params(a, params),
            Type::TypeRef(..) | Type::This(_) | Type::Package(_) => true,
            _ => false,
        }
    }

    /// The bounds of type `s`: its GADT bounds, else its declared ones.
    pub fn current_bounds(&mut self, s: SymbolId) -> Option<(TypeId, TypeId)> {
        if let Some(b) = self.u.gadt_bounds(s) {
            return Some(b);
        }
        let info = self.u.info(s);
        match *self.u.types().get(info) {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            _ => None,
        }
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

    /// Whether `a` conforms to `b` without the GADT bounds, frozen.
    pub fn conforms_without_gadt(&mut self, a: TypeId, b: TypeId) -> bool {
        let (a, b) = (self.zonk(a), self.zonk(b));
        let narrowed = self.gadt_narrowed();
        let current: Vec<_> = narrowed.iter().map(|&s| (s, self.u.set_gadt_bounds(s, None))).collect();
        let ok = self.u.is_subtype(a, b);
        for (s, bounds) in current {
            self.u.set_gadt_bounds(s, bounds);
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
