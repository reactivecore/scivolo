// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! An overloaded method against the `apply` members of other alternatives' results (dotty's
//! `resolveOverloaded` with `applyMembers`, `compare`).

use super::Resolution;
use super::applicability::rigid_param_types;
use super::named::applicable_to_proto;
use crate::typer::Typer;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// An `apply` member of the result of a parameterless alternative, as `apply` of `f` in
/// `def f: Int => Int`.
#[derive(Clone, Copy)]
pub struct ApplyMember {
    /// The `apply` method.
    pub apply: SymbolId,
    /// The result type `apply` is a member of.
    pub value: TypeId,
    /// The type of the alternative.
    pub info: TypeId,
}

impl Resolution {
    /// Checks whether method `m` applies to the arguments of `proto` and is better than the
    /// `apply` members `applies`, as `def f(x: Int)` against `def f: Int => Int`: by its owner,
    /// a module whose companion class derives from theirs (dotty's `compareOwner`), and as
    /// specific as each of them.
    pub fn wins_over_applies(
        self,
        t: &mut Typer,
        m: SymbolId,
        applies: &[ApplyMember],
        proto: usize,
    ) -> bool {
        let Some(owner) = t.u.symbol(m).owner else { return false };
        let class = match t.u.is_module_class(owner) {
            true => t.u.companion_class(owner),
            false => None,
        };
        for &member in applies {
            let Some(a_owner) = t.u.symbol(member.apply).owner else { return false };
            let derives = |t: &mut Typer, c: SymbolId| c != a_owner && t.u.derives_from(c, a_owner);
            let better = derives(t, owner) || class.is_some_and(|c| derives(t, c));
            if !better
                || t.u.derives_from(a_owner, owner)
                || t.u.is_module_class(a_owner)
                || !self.apply_takes_params_of(t, m, member)
            {
                return false;
            }
        }
        let Ok(arg_types) = self.proto_arg_types(t, proto, &[m]) else { return false };
        let ctx = t.protos[proto].ctx;
        applicable_to_proto(t, m, self.prefix, (proto, &arg_types), false, ctx) == Some(true)
    }

    /// Checks whether the `apply` of `member` takes the parameter types of method `m` (dotty's
    /// `isAsSpecific(m, apply)`); a type parameter of the alternative bounded by `Any` takes any
    /// type, any other part of its parameters none.
    fn apply_takes_params_of(self, t: &mut Typer, m: SymbolId, member: ApplyMember) -> bool {
        let ApplyMember { apply, value, info } = member;
        let Some((pm, false)) = rigid_param_types(t, m, self.prefix) else { return false };
        let seen = t.u.member_info(value, apply);
        let seen = t.resolve_class_params(seen);
        let Some((_, pa)) = t.u.param_clauses(seen).into_iter().next() else { return false };
        if pa.len() != pm.len() {
            return false;
        }

        let poly = match t.u.types().get(info) {
            Type::Poly { binder, params, .. } => Some((*binder, params.clone())),
            _ => None,
        };
        let alt_param = |u: &mut Universe, x: TypeId| match *u.types().get(x) {
            Type::Sym(None, s) if u.symbol(s).kind == Kind::TypeParam => {
                u.symbol(s).owner.is_some_and(|o| u.info(o) == info).then(|| u.info(s))
            }
            _ => None,
        };
        for (&p, &a) in pm.iter().zip(&pa) {
            let bounds = match *t.u.types().get(a) {
                Type::Param(b, i) => poly.as_ref().filter(|(pb, _)| *pb == b).map(|(_, ps)| ps[i as usize].1),
                _ => alt_param(t.u, a),
            };
            if let Some(bounds) = bounds {
                let Type::Bounds(_, hi) = *t.u.types().get(bounds) else { return false };
                if !t.is_exactly_any(hi) {
                    return false;
                }
                continue;
            }
            let mut mentions = false;
            t.u.map_type(a, &mut |u, part| {
                mentions |= matches!(u.types().get(part), Type::Param(b, _) if poly.as_ref().is_some_and(|(pb, _)| pb == b))
                    || alt_param(u, part).is_some();
                None
            });
            if mentions || !t.types().conforms_unlifted(p, a) {
                return false;
            }
        }
        true
    }
}
