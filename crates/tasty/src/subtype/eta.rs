// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//! Type constructors: how many type parameters they take (dotty's `typeParams`) and as type
//! lambdas (`etaExpand`).

use super::Cmp;
use super::refs::Meaning;
use crate::FileId;
use crate::types::{Binder, Type, TypeId};

/// The file of the binders of eta-expansions, numbered by their constructor.
const ETA: u32 = u32::MAX - 2;
/// How many upper bounds `hk_arity` follows.
const MAX_BOUNDS: usize = 16;

impl Cmp<'_> {
    /// How many type parameters type constructor `t` takes: a type lambda's, or those of a class
    /// referred to without arguments.
    pub fn constructor_arity(&mut self, t: TypeId) -> Option<usize> {
        if let Some(l) = self.u.lambda(t) {
            return Some(l.params.len());
        }
        let Some(Meaning::Class(c)) = self.named(t).map(|n| n.meaning) else { return None };
        let n = self.u.type_params(c).len();
        (n > 0).then_some(n)
    }

    /// `constructor_arity`, or that of an abstract type's upper bound (dotty's `typeParams`).
    pub fn hk_arity(&mut self, t: TypeId) -> Option<usize> {
        let mut t = t;
        for _ in 0..MAX_BOUNDS {
            match self.named(t).map(|n| n.meaning) {
                Some(Meaning::Abstract(_, hi)) if hi != t => t = hi,
                _ => return self.constructor_arity(t),
            }
        }
        None
    }

    /// Class constructor `t`, which takes type parameters, or abstract type `t` with a type
    /// lambda upper bound, as a type lambda applying it to them, their bounds referring to the
    /// lambda's parameters (dotty's `etaExpand`).
    pub fn eta_expand(&mut self, t: TypeId) -> Option<TypeId> {
        let c = match self.named(t).map(|n| n.meaning) {
            Some(Meaning::Class(c)) if self.constructor_arity(t).is_some() => c,
            Some(Meaning::Abstract(_, hi)) => return self.abstract_eta_expand(t, hi),
            _ => return None,
        };
        let params = self.u.type_params(c);
        let binder = Binder(FileId(ETA), t.0);
        let refs: Vec<TypeId> =
            (0..params.len() as u32).map(|i| self.u.intern_type(Type::Param(binder, i))).collect();
        let mut lambda_params = Vec::new();
        for &p in &params {
            let info = self.u.info(p);
            let bounds = self.u.map_type(info, &mut |u, x| match u.class_param(x) {
                Some((owner, i)) if owner == c => refs.get(i).copied(),
                _ => None,
            });
            lambda_params.push((self.u.interned_name(p), bounds));
        }
        let result = self.u.intern_applied(t, &refs);
        Some(self.u.intern_lambda(binder, &lambda_params, result))
    }

    fn abstract_eta_expand(&mut self, t: TypeId, hi: TypeId) -> Option<TypeId> {
        let l = self.u.lambda(hi)?;
        let (own, params) = (l.binder, l.params.to_vec());
        let binder = Binder(FileId(ETA), t.0);
        let refs: Vec<TypeId> =
            (0..params.len() as u32).map(|i| self.u.intern_type(Type::Param(binder, i))).collect();
        let names: Vec<_> = params.iter().map(|&(n, _)| n).collect();
        let params: Vec<_> =
            params.into_iter().map(|(n, b)| (n, self.u.subst(b, own, &names, &refs))).collect();
        let result = self.u.intern_applied(t, &refs);
        Some(self.u.intern_lambda(binder, &params, result))
    }
}
