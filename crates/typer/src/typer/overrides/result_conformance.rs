// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Whether an overriding member's result type conforms to the overridden one's (dotty's
//! `Type.overrides` of `checkOverride`, with its `upwardsSelf` retry).

use super::super::Typer;
use tasty::{Binder, NameId, SymbolId, Type, TypeId};

/// Whether the result type of member `d` conforms to that of member `m`, both seen from the
/// class of `site`'s `this` or else from `site`, with `m`'s type and value parameters renamed to
/// `d`'s.
pub fn result_conforms(t: &mut Typer<'_>, site: TypeId, d: SymbolId, m: SymbolId) -> bool {
    let this = t.u.intern_type(Type::This(site));
    results_conform(t, this, d, m) || results_conform(t, site, d, m)
}

/// A parameter clause of a method type.
struct Clause {
    is_type: bool,
    binder: Binder,
    params: Vec<(NameId, TypeId)>,
    result: TypeId,
}

/// Whether the result type of member `d` conforms to that of member `m`, both seen from `prefix`,
/// else with abstract types replaced by the aliases overriding them.
fn results_conform(t: &mut Typer<'_>, prefix: TypeId, d: SymbolId, m: SymbolId) -> bool {
    let (mut rd, mut rm) = (t.u.own_member_info(prefix, d), t.u.own_member_info(prefix, m));
    let (td, tm) = (t.u.type_params(d), t.u.type_params(m));
    let mut offset = 0;
    loop {
        match (clause(t, rd), clause(t, rm)) {
            (Some(a), Some(b)) if a.is_type == b.is_type && a.params.len() == b.params.len() => {
                let rb = t.u.param_syms_as_refs(b.result, &b.params, b.binder);
                rm = t.u.with_binder(rb, (b.binder, &b.params), a.binder);
                rd = t.u.param_syms_as_refs(a.result, &a.params, a.binder);
                if a.is_type {
                    let n = a.params.len();
                    rd = type_param_refs(t, rd, td.get(offset..offset + n).unwrap_or_default(), a.binder);
                    rm = type_param_refs(t, rm, tm.get(offset..offset + n).unwrap_or_default(), a.binder);
                    offset += n;
                }
            }
            (Some(a), _) if !a.is_type && a.params.is_empty() => rd = a.result,
            (_, Some(b)) if !b.is_type && b.params.is_empty() => rm = b.result,
            _ => break,
        }
    }

    let (rd, rm) = (widen_expr(t, rd), widen_expr(t, rm));
    if t.conforms(rd, rm) {
        return true;
    }
    let overridden = |t: &mut Typer<'_>, ty: TypeId| t.u.map_type(ty, &mut |u, x| u.overriding_member(x));
    let (rd, rm) = (overridden(t, rd), overridden(t, rm));
    t.conforms(rd, rm)
}

/// The leading parameter clause of method type `info`, if it has one.
fn clause(t: &Typer<'_>, info: TypeId) -> Option<Clause> {
    let (is_type, binder, params, result) = match *t.u.ty(info) {
        Type::Poly { binder, params, result } => (true, binder, params, result),
        Type::Method { binder, params, result, .. } => (false, binder, params, result),
        _ => return None,
    };
    Some(Clause { is_type, binder, params: t.u.params_of(params).to_vec(), result })
}

/// `ty` with references to type parameters `params` made references to parameters of `binder`.
fn type_param_refs(t: &mut Typer<'_>, ty: TypeId, params: &[SymbolId], binder: Binder) -> TypeId {
    t.u.map_type(ty, &mut |u, x| {
        let Some((None, s)) = u.sym_ref(x) else { return None };
        params.iter().position(|&p| p == s).map(|i| u.intern_type(Type::Param(binder, i as u32)))
    })
}

/// `T` for `=> T`, else `ty`.
fn widen_expr(t: &Typer<'_>, ty: TypeId) -> TypeId {
    match *t.u.ty(ty) {
        Type::ByName(r) => r,
        _ => ty,
    }
}
