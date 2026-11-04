// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Higher-kinded types: a type variable applied to arguments, constrained to a constructor of
//! the other side (dotty's `compareAppliedTypeParamRef`), constructors combined under a lambda
//! (`liftIfHK`), and tuple classes as nested pairs.

use super::Cmp;
use crate::types::{Binder, Type, TypeId};
use crate::{ANY, FileId, Kind, NOTHING, SymbolId};

/// The files of the binders of adapted constructors (numbered by the constructor they apply)
/// and of combined constructors.
const ADAPTED: u32 = u32::MAX - 3;
const LIFTED: u32 = u32::MAX - 4;

/// `&`, or `|` making a soft union if `.0`.
#[derive(Clone, Copy, PartialEq)]
pub enum Connective {
    And,
    Or(bool),
}

impl Cmp<'_> {
    /// `a` and `b` combined by `op`; for two constructors of as many parameters, a type lambda
    /// of that of their applications (dotty's `liftIfHK`).
    pub fn lift_if_constructors(&mut self, a: TypeId, b: TypeId, op: Connective) -> TypeId {
        let combined = match (self.hk_arity(a), self.hk_arity(b)) {
            (Some(n), Some(m)) if n == m => n,
            _ => return self.balanced(a, b, op),
        };
        let binder = Binder(FileId(LIFTED), a.0.wrapping_mul(0x9e37_79b9) ^ b.0);
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        let params: Vec<(String, TypeId)> = (0..combined).map(|i| (format!("X{i}"), bounds)).collect();
        let refs: Vec<TypeId> =
            (0..combined as u32).map(|i| self.u.intern_type(Type::Param(binder, i))).collect();
        let (applied_a, applied_b) = (self.apply(a, &refs), self.apply(b, &refs));
        let result = match op {
            Connective::And => self.glb(applied_a, applied_b),
            Connective::Or(soft) => self.lub_as(applied_a, applied_b, soft),
        };
        self.u.intern_type(Type::Lambda { binder, params, result })
    }

    /// `a` and `b` combined by `op`, rebalanced if one side has more than one part more than
    /// the other (dotty's `AndType.balanced`, `OrType.balanced`).
    fn balanced(&mut self, a: TypeId, b: TypeId, op: Connective) -> TypeId {
        if let Connective::Or(soft) = op {
            return self.u.union(a, b, soft);
        }
        let mut parts = Vec::new();
        self.flattened(a, &mut parts);
        let left = parts.len();
        self.flattened(b, &mut parts);
        if left.abs_diff(parts.len() - left) <= 1 {
            return self.u.intern_type(Type::And(a, b));
        }
        self.built(&parts)
    }

    fn flattened(&self, t: TypeId, out: &mut Vec<TypeId>) {
        match *self.u.types().get(t) {
            Type::And(x, y) => [x, y].into_iter().for_each(|p| self.flattened(p, out)),
            _ => out.push(t),
        }
    }

    fn built(&mut self, parts: &[TypeId]) -> TypeId {
        if let [t] = parts {
            return *t;
        }
        let (x, y) = parts.split_at(parts.len() / 2);
        let (x, y) = (self.built(x), self.built(y));
        self.u.intern_type(Type::And(x, y))
    }

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
        let Type::Var(v) = *self.u.types().get(var) else { return false };
        let class_bounds = self.class_symbols(self.vars.bounds(v).1);
        for base in self.u.linearization(class) {
            if (class_bounds.is_empty() || class_bounds.iter().any(|&c| self.u.derives_from(base, c)))
                && let Some(base_type) = self.u.base_type(widened, base)
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
        if tycon == var {
            return false;
        }
        let adapted = match fixed {
            0 => tycon,
            _ => {
                let binder = Binder(FileId(ADAPTED), tycon.0);
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

    /// `(A, B, …)` for tuple class `c` applied to `args` as `A *: B *: … *: EmptyTuple`.
    pub fn nested_pairs(&mut self, c: SymbolId, args: &[TypeId]) -> Option<TypeId> {
        if !self.u.symbol(c).name.starts_with("Tuple") {
            return None;
        }
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
