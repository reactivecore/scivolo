//! Higher-kinded comparisons: a type variable applied to arguments, constrained to a
//! constructor of the other side (dotty's `compareAppliedTypeParamRef`), class constructors as
//! type lambdas (`etaExpand`), and tuple classes as nested pairs.

use super::Cmp;
use super::refs::Meaning;
use crate::types::{Type, TypeId};
use crate::{ANY, Kind, NOTHING};

impl Cmp<'_> {
    /// `a <: v[args]` for variable constructor `v` without instance: `v` above the constructor
    /// of `a`, or of a base type of `a`, applied to the arguments `args` leave over.
    pub fn var_constructor_above(&mut self, a: TypeId, var: TypeId, args: &[TypeId]) -> bool {
        if self.frozen() {
            return false;
        }
        let widened = self.u.widen(a);
        if self.unify_constructor(var, args, widened, true) {
            return true;
        }
        let Some(class) = self.u.class_symbol(widened) else { return false };
        for base in self.u.linearization(class).into_iter().skip(1) {
            if let Some(base_type) = self.u.base_type(widened, base)
                && self.unify_constructor(var, args, base_type, true)
            {
                return true;
            }
        }
        false
    }

    /// `v[args] <: b` for variable constructor `v` without instance: `v` below the constructor
    /// of `b`.
    pub fn var_constructor_below(&mut self, var: TypeId, args: &[TypeId], b: TypeId) -> bool {
        !self.frozen() && self.unify_constructor(var, args, b, false)
    }

    /// Whether variable `var` can take the constructor of application `other`, its leading
    /// arguments fixed, so that `var[args]` is below `other` (above it if `from_below`); the
    /// constraint as before if not.
    fn unify_constructor(&mut self, var: TypeId, args: &[TypeId], other: TypeId, from_below: bool) -> bool {
        let Type::Applied(tycon, other_args) = self.u.types().get(other).clone() else { return false };
        let Some(fixed) = other_args.len().checked_sub(args.len()) else { return false };
        let adapted = match fixed {
            0 => tycon,
            _ => {
                let binder = self.u.fresh_binder();
                let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
                let params: Vec<(String, TypeId)> =
                    (0..args.len()).map(|i| (format!("X{i}"), bounds)).collect();
                let mut body_args = other_args[..fixed].to_vec();
                body_args.extend((0..args.len() as u32).map(|i| self.u.intern_type(Type::Param(binder, i))));
                let result = self.u.intern_type(Type::Applied(tycon, body_args));
                self.u.intern_type(Type::Lambda { binder, params, result })
            }
        };
        let mark = self.vars.mark();
        let constructor = match from_below {
            true => self.recur(adapted, var),
            false => self.recur(var, adapted),
        };
        let applied = self.apply(adapted, args);
        let unified = constructor
            && match from_below {
                true => self.recur(other, applied),
                false => self.recur(applied, other),
            };
        if !unified {
            self.vars.rollback(mark);
        }
        unified
    }

    /// Class constructor `t`, which takes type parameters, as a type lambda applying it to
    /// them (dotty's `etaExpand`).
    pub fn eta_expand(&mut self, t: TypeId) -> Option<TypeId> {
        let Some(Meaning::Class(c)) = self.named(t).map(|n| n.meaning) else { return None };
        let params = self.u.type_params(c);
        if params.is_empty() {
            return None;
        }
        let binder = self.u.fresh_binder();
        let lambda_params: Vec<(String, TypeId)> =
            params.iter().map(|&p| (self.u.symbol(p).name.clone(), self.u.info(p))).collect();
        let refs: Vec<TypeId> =
            (0..params.len() as u32).map(|i| self.u.intern_type(Type::Param(binder, i))).collect();
        let result = self.u.intern_type(Type::Applied(t, refs));
        Some(self.u.intern_type(Type::Lambda { binder, params: lambda_params, result }))
    }

    /// `(A, B, …)` for tuple class `c` applied to `args` as `A *: B *: … *: EmptyTuple`.
    pub fn nested_pairs(&mut self, c: crate::SymbolId, args: &[TypeId]) -> Option<TypeId> {
        let full = self.u.full_name(c);
        let arity: usize = full.strip_prefix("scala.Tuple")?.parse().ok()?;
        if arity != args.len() {
            return None;
        }
        let scala = self.u.package("scala")?;
        let found = |s: &mut Self, name: &str, kind: Kind| {
            let decl = s.u.decl(scala, name).ok()?;
            decl.into_iter().find(|&d| s.u.symbol(d).kind == kind)
        };
        let (pair, empty) = (found(self, "*:", Kind::Class)?, found(self, "EmptyTuple", Kind::Type)?);
        let (pair, mut t) =
            (self.u.intern_type(Type::Sym(None, pair)), self.u.intern_type(Type::Sym(None, empty)));
        for &arg in args.iter().rev() {
            t = self.u.intern_type(Type::Applied(pair, vec![arg, t]));
        }
        Some(t)
    }
}
