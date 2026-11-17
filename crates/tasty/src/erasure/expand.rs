// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Applied aliases and abstract types, whose arguments are substituted into their right-hand
//! side or upper bound.

use crate::types::{NameId, Type, TypeId};
use crate::{Kind, Universe};

impl Universe {
    /// `tycon[args]` with `tycon` an alias of a type lambda, also by a refinement of its prefix's
    /// type (`f.A[Int]` of `f: R` with `R <: Foo { type A[T] = Long }`): its body as seen from
    /// the prefix of `tycon`, arguments substituted.
    pub fn expand(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        self.applied_info(tycon, args, false)
    }

    /// `expand`, or for `tycon` an abstract type bounded by a type lambda, that lambda's body,
    /// met with the one a refinement of its prefix's type bounds it by (dotty's `superType` of an
    /// applied type).
    pub fn expand_upper(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        self.applied_info(tycon, args, true)
    }

    fn applied_info(&mut self, tycon: TypeId, args: &[TypeId], upper: bool) -> Option<TypeId> {
        let (prefix, alias) = match *self.ty(tycon) {
            Type::Sym(p, s) => (p, Some(s)),
            Type::TypeRef(p, n) => (Some(p), self.resolve(p, n, true)),
            _ => (None, None),
        };
        let alias = alias?;
        let kind = self.symbol(alias).kind;
        if kind != Kind::Type && !(upper && kind == Kind::TypeParam) {
            return None;
        }

        let refined = prefix.and_then(|p| self.refinement_info(p, alias));
        if let Some(refined) = refined.and_then(|r| self.type_alias(r)) {
            return self.lambda_applied(refined, args, upper);
        }

        let own = match prefix {
            Some(p) => self.member_info(p, alias),
            None => self.info(alias),
        };
        let own = self.lambda_applied(own, args, upper);
        match (refined.filter(|_| upper).and_then(|r| self.lambda_applied(r, args, upper)), own) {
            (Some(r), Some(o)) => {
                let o = self.bounds(o).map_or(o, |(_, hi)| hi);
                Some(self.glb(r, o))
            }
            (r, o) => o.or(r),
        }
    }

    /// The body of type lambda `info`, or with `upper` of its upper bound, arguments `args`
    /// substituted.
    fn lambda_applied(&mut self, info: TypeId, args: &[TypeId], upper: bool) -> Option<TypeId> {
        let info = self.bounds(info).filter(|_| upper).map_or(info, |(_, hi)| hi);
        let (binder, names, result) = self.lambda(info).map(|l| (l.binder, l.names(), l.result))?;
        Some(self.subst(result, binder, &names, args))
    }

    /// `t` with the parameters of lambda `binder`, named `names`, replaced by `args`.
    pub fn subst(&mut self, t: TypeId, binder: crate::Binder, names: &[NameId], args: &[TypeId]) -> TypeId {
        self.map_type(t, &mut |u, x| match *u.ty(x) {
            Type::Local(b, n) if b.0 == binder.0 => {
                names.iter().position(|&m| m == n).and_then(|i| args.get(i).copied())
            }
            Type::Param(b, i) if b == binder => args.get(i as usize).copied(),
            _ => None,
        })
    }
}
