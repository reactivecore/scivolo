// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Whether an overriding member's result type conforms to the overridden one's (dotty's
//! `Type.overrides` of `checkOverride`, with its `upwardsSelf` retry).

use super::super::Typer;
use crate::symbols::Completion;
use tasty::{Binder, Kind, NameId, SymbolId, Type, TypeId, Universe};

/// Whether the result type of member `d` conforms to that of member `m`, both seen from the
/// class of `site`'s `this` or else from `site`, with `m`'s type and value parameters renamed to
/// `d`'s.
pub fn result_conforms(t: &mut Typer<'_>, site: TypeId, d: SymbolId, m: SymbolId) -> bool {
    let this = t.u.intern_type(Type::This(site));
    results_conform(t, (site, this), d, m) || results_conform(t, (site, site), d, m)
}

/// A parameter clause of a method type.
struct Clause {
    is_type: bool,
    binder: Binder,
    params: Vec<(NameId, TypeId)>,
    result: TypeId,
}

/// Whether the result type of member `d` conforms to that of member `m`, both seen from `prefix`
/// in the class of `site`, else with abstract types replaced by the aliases overriding them.
fn results_conform(t: &mut Typer<'_>, (site, prefix): (TypeId, TypeId), d: SymbolId, m: SymbolId) -> bool {
    let (mut rd, mut rm) = (t.u.own_member_info(prefix, d), t.u.own_member_info(prefix, m));
    let (td, tm) = (t.u.type_params(d), t.u.type_params(m));
    let (mut offset, mut bounds) = (0, Vec::new());
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
                    for (i, &(_, info)) in a.params.iter().enumerate() {
                        bounds.push((t.u.intern_type(Type::Param(a.binder, i as u32)), info));
                    }
                    offset += n;
                }
            }
            (Some(a), _) if !a.is_type && a.params.is_empty() => rd = a.result,
            (_, Some(b)) if !b.is_type && b.params.is_empty() => rm = b.result,
            _ => break,
        }
    }

    let (rd, rm) = (widen_expr(t, rd), widen_expr(t, rm));
    let (rd, rm) = (strip_annotations(t.u, rd), strip_annotations(t.u, rm));
    if t.conforms(rd, rm) {
        return true;
    }
    complete_overriders(t, site, &[rd, rm]);
    let overridden = |t: &mut Typer<'_>, ty: TypeId| t.u.map_type(ty, &mut |u, x| u.overriding_member(x));
    let (rd, rm) = (overridden(t, rd), overridden(t, rm));
    let bound = |t: &Typer<'_>, ty: TypeId, upper: bool| {
        let info = bounds.iter().find(|&&(p, _)| p == ty).map(|&(_, info)| info);
        info.and_then(|i| t.u.bounds(i)).map_or(ty, |(lo, hi)| if upper { hi } else { lo })
    };
    let (rd, rm) = (bound(t, rd, true), bound(t, rm, false));
    t.conforms(rd, rm) || has_lambda(t, rd) || has_lambda(t, rm)
}

/// Whether `ty` has a type lambda, as a higher-kinded type argument, which the check accepts.
fn has_lambda(t: &mut Typer<'_>, ty: TypeId) -> bool {
    let mut found = false;
    t.u.map_type(ty, &mut |u, x| {
        found |= matches!(*u.ty(x), Type::Lambda { .. });
        None
    });
    found
}

/// `ty` without its annotations, as capture sets.
fn strip_annotations(u: &mut Universe, ty: TypeId) -> TypeId {
    u.map_type(ty, &mut |u, x| match *u.ty(x) {
        Type::Annotated(inner, _) => Some(strip_annotations(u, inner)),
        _ => None,
    })
}

/// Completes the type members of the unit in the base classes of `site`'s class named as the
/// abstract types `types` refer to on a prefix.
fn complete_overriders(t: &mut Typer<'_>, site: TypeId, types: &[TypeId]) {
    let mut names = Vec::new();
    for &ty in types {
        t.u.map_type(ty, &mut |u, x| {
            if let Type::Sym(Some(_), s) = *u.ty(x)
                && u.symbol(s).kind == Kind::Type
            {
                names.push(u.symbol(s).name.clone());
            }
            None
        });
    }
    let Some(cls) = t.u.class_symbol(site) else { return };
    for b in t.u.linearization(cls) {
        let decls = t.u.symbol(b).decls.clone();
        for d in decls {
            let pending = matches!(t.syms.completions.get(&d), Some(Completion::Pending { .. }));
            if pending && t.u.symbol(d).kind == Kind::Type && names.contains(&t.u.symbol(d).name) {
                t.complete(d);
            }
        }
    }
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
