//! Pattern-bound types avoided in a case body's type (dotty's `avoid` of an abstract type that
//! is an argument of an invariant class type parameter).

use super::Typer;
use tasty::{Kind, NOTHING, SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Applied class type `t` with each argument at an invariant parameter that is one of the
    /// abstract types `locals`, or a type variable that would be instantiated to one, replaced
    /// by its bounds; `t` itself otherwise.
    pub fn avoid_pattern_types(&mut self, t: TypeId, locals: &[SymbolId]) -> TypeId {
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return t };
        let Type::Sym(_, class) = *self.u.types().get(tycon) else { return t };
        if self.u.symbol(class).kind != Kind::Class {
            return t;
        }
        let variances = self.u.variances(class);
        let mut avoided = Vec::with_capacity(args.len());
        for (i, a) in args.into_iter().enumerate() {
            let arg = match variances.get(i) {
                Some(0) => self.local_bounds(a, locals).unwrap_or(a),
                _ => a,
            };
            avoided.push(arg);
        }
        self.u.intern_type(Type::Applied(tycon, avoided))
    }

    /// The bounds of abstract type `t` if it is one of `locals`, or of the one an uninstantiated
    /// type variable `t` would be instantiated to (dotty's `instanceType` in `AvoidMap`).
    fn local_bounds(&mut self, t: TypeId, locals: &[SymbolId]) -> Option<TypeId> {
        let t = match *self.u.types().get(t) {
            Type::Var(v) if self.constraint.instance(v).is_none() => {
                let (lo, hi) = self.constraint.bounds(v);
                if lo != NOTHING { lo } else { hi }
            }
            _ => t,
        };
        let Type::Sym(_, s) = *self.u.types().get(t) else { return None };
        let info = self.u.info(s);
        (locals.contains(&s) && matches!(self.u.types().get(info), Type::Bounds(..))).then_some(info)
    }

    /// Whether an uninstantiated type variable in `t` has a bound that refers to an abstract type
    /// among `locals`.
    pub fn var_bounds_refer_to(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        let mut syms = Vec::new();
        for &s in locals {
            let info = self.u.info(s);
            if self.u.symbol(s).kind == Kind::Type && matches!(self.u.types().get(info), Type::Bounds(..)) {
                syms.push(s);
            }
        }
        if syms.is_empty() {
            return false;
        }
        let mut vars = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part) {
                vars.push(v);
            }
            None
        });
        vars.into_iter().any(|v| {
            let (lo, hi) = self.constraint.bounds(v);
            self.constraint.instance(v).is_none() && (self.refers_to(lo, &syms) || self.refers_to(hi, &syms))
        })
    }
}
