// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! An overloaded method against the `apply` members of other alternatives' results (dotty's
//! `resolveOverloaded` with `applyMembers`, `compare`).

use super::Typer;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether method `m`, a member of `prefix`, applies to the arguments of `proto` and is
    /// better than the `apply` members `applies` (each with the result type it is a member of and
    /// the type of that result's alternative) by its owner, a module whose companion class
    /// derives from theirs (dotty's `compareOwner`), and as specific as each of them.
    pub fn wins_over_applies(
        &mut self,
        m: SymbolId,
        applies: &[(SymbolId, TypeId, TypeId)],
        prefix: Option<TypeId>,
        proto: usize,
    ) -> bool {
        let Some(owner) = self.u.symbol(m).owner else { return false };
        let class = match self.u.is_module_class(owner) {
            true => self.u.companion_class(owner),
            false => None,
        };
        for &(a, value, info) in applies {
            let Some(a_owner) = self.u.symbol(a).owner else { return false };
            let derives = |t: &mut Self, c: SymbolId| c != a_owner && t.u.derives_from(c, a_owner);
            let better = derives(self, owner) || class.is_some_and(|c| derives(self, c));
            if !better
                || self.u.derives_from(a_owner, owner)
                || self.u.is_module_class(a_owner)
                || !self.apply_takes_params_of(m, prefix, (a, value, info))
            {
                return false;
            }
        }
        let Ok(arg_types) = self.proto_arg_types(proto, &[m], prefix) else { return false };
        let ctx = self.protos[proto].ctx;
        self.applicable_to_proto(m, prefix, (proto, &arg_types), false, ctx) == Some(true)
    }

    /// Whether `apply`, a member of `value`, the result of an alternative of type `info`, takes
    /// the parameter types of method `m` (dotty's `isAsSpecific(m, apply)`); a type parameter of
    /// `info` bounded by `Any` takes any type, any other part of `info`'s parameters none.
    fn apply_takes_params_of(
        &mut self,
        m: SymbolId,
        prefix: Option<TypeId>,
        (apply, value, info): (SymbolId, TypeId, TypeId),
    ) -> bool {
        let Some((pm, false)) = self.rigid_param_types(m, prefix) else { return false };
        let seen = self.u.member_info(value, apply);
        let seen = self.resolve_class_params(seen);
        let Some((_, pa)) = self.u.param_clauses(seen).into_iter().next() else { return false };
        if pa.len() != pm.len() {
            return false;
        }
        let poly = match self.u.types().get(info) {
            Type::Poly { binder, params, .. } => Some((*binder, params.clone())),
            _ => None,
        };
        let alt_param = |u: &mut tasty::Universe, x: TypeId| match *u.types().get(x) {
            Type::Sym(None, s) if u.symbol(s).kind == Kind::TypeParam => {
                u.symbol(s).owner.is_some_and(|o| u.info(o) == info).then(|| u.info(s))
            }
            _ => None,
        };
        for (&p, &a) in pm.iter().zip(&pa) {
            let bounds = match *self.u.types().get(a) {
                Type::Param(b, i) => poly.as_ref().filter(|(pb, _)| *pb == b).map(|(_, ps)| ps[i as usize].1),
                _ => alt_param(self.u, a),
            };
            if let Some(bounds) = bounds {
                let Type::Bounds(_, hi) = *self.u.types().get(bounds) else { return false };
                if !self.is_exactly_any(hi) {
                    return false;
                }
                continue;
            }
            let mut mentions = false;
            self.u.map_type(a, &mut |u, part| {
                mentions |= matches!(u.types().get(part), Type::Param(b, _) if poly.as_ref().is_some_and(|(pb, _)| pb == b))
                    || alt_param(u, part).is_some();
                None
            });
            if mentions || !self.types().conforms_unlifted(p, a) {
                return false;
            }
        }
        true
    }
}
