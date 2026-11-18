// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Selections on a prefix that the approximating substitution changed.

use super::Subst;
use crate::NOTHING;
use crate::approx::{self, Approx};
use crate::symbols::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// Reference `t` with prefix `p` that mapped to `pre` (dotty's `derivedSelect` of
    /// `ApproximatingTypeMap`): on a range, what the member of the upper prefix stands for if
    /// known, as `String` for `type T = String`, else the range of the member on both prefixes:
    /// `Outer[A]#In` at an invariant place of `Sub[?]` lies between `Nothing` and `Outer[?]#In`.
    pub fn derived_select(&mut self, t: TypeId, p: TypeId, pre: Approx, v: i8, s: Subst<'_>) -> Approx {
        if pre == Approx::Exact(p) {
            return Approx::Exact(t);
        }
        let member = self.selected_member(t);
        let (lo, hi) = match pre {
            Approx::Exact(q) => return self.select_on(t, q, member, v),
            Approx::Range(lo, hi) => (lo, hi),
        };
        if let Some(m) = member
            && let Some(widened) = self.widened_member(hi, m, v, s)
        {
            return widened;
        }
        let lo = self.select_on(t, lo, member, v).lo();
        let hi = self.select_on(t, hi, member, v).hi();
        approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v)
    }

    fn selected_member(&mut self, t: TypeId) -> Option<SymbolId> {
        match *self.ty(t) {
            Type::Sym(_, s) => Some(s),
            _ => self.resolve_ref(t),
        }
    }

    /// `t` selected on `q` (dotty's `NamedType.derivedSelect`): `Nothing` on `Nothing`, the
    /// argument `q` gives a class type parameter, the alias a refinement of `q` gives the name.
    fn select_on(&mut self, t: TypeId, q: TypeId, member: Option<SymbolId>, v: i8) -> Approx {
        if q == NOTHING {
            return Approx::Exact(NOTHING);
        }
        let node = *self.ty(t);
        if let Some(m) = member {
            if let Some(arg) = self.arg_for_param(q, m) {
                return self.approx_arg(arg, v);
            }
            let name = self.symbol(m).name.clone();
            if let Some(alias) = self.lookup_refined(q, &name) {
                return Approx::Exact(alias);
            }
        }
        Approx::Exact(self.intern_type(match node {
            Type::Sym(_, s) => Type::Sym(Some(q), s),
            Type::TypeRef(_, n) => Type::TypeRef(q, n),
            Type::TermRef(_, n) => Type::TermRef(q, n),
            other => other,
        }))
    }

    /// What member `m` of `hi`, an upper prefix, stands for under every prefix below `hi` (dotty's
    /// `expandParam`, `tryWiden`): the argument of a class type parameter, the alias or bounds of
    /// a type member, the singleton type of a term member.
    fn widened_member(&mut self, hi: TypeId, m: SymbolId, v: i8, s: Subst<'_>) -> Option<Approx> {
        match self.symbol(m).kind {
            Kind::TypeParam => {
                let arg = self.arg_for_param(hi, m)?;
                Some(self.approximated_info(arg, v, s))
            }
            Kind::Type => {
                let refined = self.refinement_info(hi, m).or_else(|| self.overriding_bounds(hi, m));
                let info = refined.unwrap_or_else(|| self.member_info(hi, m));
                matches!(self.ty(info), Type::Alias(_) | Type::Bounds(..))
                    .then(|| self.approximated_info(info, v, s))
            }
            k if k.is_term() => {
                let info = self.member_info(hi, m);
                let singleton = match *self.ty(info) {
                    Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => true,
                    Type::Sym(_, x) => self.symbol(x).kind.is_term(),
                    _ => false,
                };
                singleton.then(|| self.approximated(info, v, s))
            }
            _ => None,
        }
    }

    /// An alias as its type, bounds as their range, anything else as it is, approximated.
    fn approximated_info(&mut self, info: TypeId, v: i8, s: Subst<'_>) -> Approx {
        match *self.ty(info) {
            Type::Alias(x) => self.approximated(x, v, s),
            Type::Bounds(lo, hi) => {
                let lo = self.approximated(lo, -v, s);
                let hi = self.approximated(hi, v, s);
                approx::range(self, lo, hi, v)
            }
            _ => self.approximated(info, v, s),
        }
    }
}
