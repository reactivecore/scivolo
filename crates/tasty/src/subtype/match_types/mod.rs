// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Normalization (dotty's `normalized`, `tryNormalize`, `underlyingNormalizable`): match types
//! reduced, `scala.compiletime.ops` applications folded.

mod concrete;
mod disjoint;
mod disjoint_args;
mod named_from;
mod ops;
mod reduce;
mod spec;

use super::{Cmp, NoVars};
use crate::Universe;
use crate::types::{Type, TypeId};

/// How many reductions may be nested.
const MAX_REDUCTIONS: u32 = 64;
/// How many strippings and beta-reductions `underlying_normalizable` takes at most.
const MAX_STEPS: u32 = 64;

impl Universe {
    /// `t` normalized, `t` itself if no normalization applies (dotty's `normalized`).
    pub fn normalized(&mut self, t: TypeId) -> TypeId {
        Cmp::new(self, &mut NoVars).normal(t)
    }

    /// `t` normalized, if a normalization applies (dotty's `tryNormalize`).
    pub fn try_normalize(&mut self, t: TypeId) -> Option<TypeId> {
        Cmp::new(self, &mut NoVars).try_normalize(t)
    }

    /// Whether match type case `case` has a pattern that reduction handles.
    pub fn is_reducible_case(&mut self, case: TypeId) -> bool {
        !matches!(Cmp::new(self, &mut NoVars).analyze(case), spec::Spec::Unsupported)
    }
}

impl Cmp<'_> {
    /// `t` normalized (dotty's `normalized`).
    pub fn normal(&mut self, t: TypeId) -> TypeId {
        self.try_normalize(t).unwrap_or(t)
    }

    pub fn try_normalize(&mut self, t: TypeId) -> Option<TypeId> {
        let n = self.underlying_normalizable(t)?;
        match self.u.types().get(n) {
            Type::Match { .. } => {
                let reduced = self.reduced(n)?;
                Some(self.normal(reduced))
            }
            _ => self.fold_compiletime(n),
        }
    }

    /// The match type or `scala.compiletime.ops` application `t` stands for, through
    /// annotations, instantiated variables and applied aliases and abstract types.
    fn underlying_normalizable(&mut self, mut t: TypeId) -> Option<TypeId> {
        for _ in 0..MAX_STEPS {
            t = match self.u.types().get(t).clone() {
                Type::Match { .. } => return Some(t),
                Type::Annotated(inner, _) => inner,
                Type::Var(v) => self.vars.instance(v)?,
                Type::Applied(tycon, args) => {
                    if self.compiletime_op(tycon).is_some() || self.is_named_tuple_from(tycon) {
                        return Some(t);
                    }
                    self.applied_super_type(tycon, &args)?
                }
                _ => return None,
            };
        }
        None
    }

    /// The supertype of `tycon[args]` for `tycon` not a class: a lambda's body, an alias's or
    /// an abstract type's upper bound applied (dotty's `AppliedType.superType`).
    pub fn applied_super_type(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        match self.u.types().get(tycon).clone() {
            Type::Lambda { .. } => Some(self.apply(tycon, args)),
            Type::Var(v) => {
                let t = self.vars.instance(v)?;
                Some(self.u.intern_type(Type::Applied(t, args.to_vec())))
            }
            Type::Annotated(inner, _) => Some(self.u.intern_type(Type::Applied(inner, args.to_vec()))),
            Type::Sym(..) | Type::TypeRef(..) => self.u.expand_upper(tycon, args),
            _ => None,
        }
    }

    /// The reduction of match type `m`, if it reduces (dotty's `MatchType.reduced`).
    pub fn reduced(&mut self, m: TypeId) -> Option<TypeId> {
        let Type::Match { scrutinee, cases, .. } = self.u.types().get(m).clone() else { return None };
        if self.reductions >= MAX_REDUCTIONS {
            return None;
        }
        self.reductions += 1;
        let frozen = std::mem::replace(&mut self.frozen, true);
        let scrutinee = self.u.dealias(scrutinee);
        let scrutinee = self.normal(scrutinee);
        let reduced = self.match_cases(scrutinee, &cases);
        self.frozen = frozen;
        self.reductions -= 1;
        reduced
    }
}
