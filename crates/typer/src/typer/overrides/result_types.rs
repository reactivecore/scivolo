// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/transform/OverridingPairs.scala
//! dotty's "has incompatible type" check of `checkOverride`: an overriding member's type must
//! conform to the overridden one's (`isOverridingPair`).

use super::super::Typer;
use super::{OverrideChecks, overridable};
use names::{name_kinds, nme};
use syntax::ast::flags as f;
use tasty::{Binder, Kind, NameId, SymbolId, Type, TypeId};

impl OverrideChecks {
    /// Whether a type member of `own`, the class's declarations, overrides a type member of a
    /// base class with bounds that do not lie within the overridden bounds, as
    /// `type T = String` overriding `type T <: Int`.
    pub fn incompatible_type_member(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> bool {
        for &d in own {
            if t.u.symbol(d).kind != Kind::Type {
                continue;
            }
            let name = t.u.symbol(d).name.clone();
            for &b in &self.bases {
                let others: Vec<SymbolId> = t.u.symbol(b).decls.iter().copied().collect();
                for m in others {
                    let sm = t.u.symbol(m);
                    if sm.name != name || sm.kind != Kind::Type || sm.is_private() {
                        continue;
                    }
                    t.complete(m);
                    if !self.bounds_within(t, d, m) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Whether the bounds of type member `d`, an alias `(a, a)`, lie within those of `m`, both
    /// seen from the class; higher-kinded members are not compared.
    fn bounds_within(&self, t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> bool {
        let this = t.u.intern_type(Type::This(self.site));
        let bounds = |t: &mut Typer<'_>, s: SymbolId| {
            let info = t.u.own_member_info(this, s);
            match *t.u.ty(info) {
                Type::Alias(a) => Some((a, a)),
                Type::Bounds(lo, hi) => Some((lo, hi)),
                _ => None,
            }
        };
        let (Some((lo1, hi1)), Some((lo2, hi2))) = (bounds(t, d), bounds(t, m)) else { return true };
        let lambda = |t: &Typer<'_>, x: TypeId| matches!(*t.u.ty(x), Type::Lambda { .. });
        if [lo1, hi1, lo2, hi2].into_iter().any(|x| lambda(t, x)) {
            return true;
        }
        t.conforms(lo2, lo1) && t.conforms(hi1, hi2)
    }

    /// Whether a term member of `own` overrides a member of a base class it matches with a result
    /// type that does not conform to the overridden one, as `def y: String` overriding
    /// `def y: Option[Int]`.
    pub fn incompatible_term_member(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> bool {
        for &d in own {
            let (kind, name) = (t.u.symbol(d).kind, t.u.symbol(d).name.clone());
            let exempt = name == nme::CONSTRUCTOR || name.contains(name_kinds::DEFAULT_GETTER);
            let private = t.syms.flags.get(&d).is_none_or(|fl| fl.is(f::PRIVATE));
            if !matches!(kind, Kind::Def | Kind::Val) || exempt || private {
                continue;
            }
            for &b in &self.bases {
                for m in overridable(t.u, b, &name) {
                    t.complete(m);
                    if t.u.symbol(d).run_name() == t.u.symbol(m).run_name()
                        && t.u.info_matches(self.site, d, m)
                        && !result_conforms(t, self.site, d, m)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Whether the result type of member `d` conforms to that of member `m`, both seen from the
/// class of `site` with `m`'s type and value parameters renamed to `d`'s.
pub fn result_conforms(t: &mut Typer<'_>, site: TypeId, d: SymbolId, m: SymbolId) -> bool {
    let this = t.u.intern_type(Type::This(site));
    let (id, im) = (t.u.own_member_info(this, d), t.u.own_member_info(this, m));
    let binder = t.u.poly(id).map(|p| p.binder);
    let (mut rd, mut rm) = t.u.aligned_results((d, id), (m, im));
    let params = t.u.type_params(d);
    if let Some(binder) = binder {
        rm = t.u.map_type(rm, &mut |u, x| {
            let Some((None, s)) = u.sym_ref(x) else { return None };
            params.iter().position(|&p| p == s).map(|i| u.intern_type(Type::Param(binder, i as u32)))
        });
    }
    for ((b1, p1), (b2, p2)) in value_clauses(t, id).into_iter().zip(value_clauses(t, im)) {
        rd = t.u.param_syms_as_refs(rd, &p1, b1);
        rm = t.u.param_syms_as_refs(rm, &p2, b2);
        rm = t.u.with_binder(rm, (b2, &p2), b1);
    }
    let (rd, rm) = (widen_expr(t, rd), widen_expr(t, rm));
    t.conforms(rd, rm)
}

/// The binder and parameters of each value parameter clause of method type `info`.
fn value_clauses(t: &Typer<'_>, mut info: TypeId) -> Vec<(Binder, Vec<(NameId, TypeId)>)> {
    let mut out = Vec::new();
    while let Type::Poly { result, .. } | Type::Method { result, .. } = *t.u.ty(info) {
        if let Type::Method { binder, params, .. } = *t.u.ty(info) {
            out.push((binder, t.u.params_of(params).to_vec()));
        }
        info = result;
    }
    out
}

/// `T` for `=> T`, else `t`.
fn widen_expr(t: &Typer<'_>, ty: TypeId) -> TypeId {
    match *t.u.ty(ty) {
        Type::ByName(r) => r,
        _ => ty,
    }
}
