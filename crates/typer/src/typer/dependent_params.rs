// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Substituting arguments for the parameters of a method type (dotty's `safeSubstParam`).

use super::stability::is_stable_path;
use tasty::{Binder, Kind, NameId, SymbolId, Type, TypeId, Universe};

/// Result `t` of the method type of binder `binder` and parameters `names`, of method `method`,
/// applied to arguments of types `args`: its references to the parameters replaced by the
/// arguments' types, a skolem of the widened type for an argument that is not stable (dotty's
/// `safeSubstParams`).
pub fn applied_result(
    u: &mut Universe,
    t: TypeId,
    (binder, names): (Binder, &[NameId]),
    method: Option<SymbolId>,
    args: &[TypeId],
) -> TypeId {
    if !mentions_clause_params(u, t, (binder, names), method) {
        return t;
    }
    let param = |u: &Universe, part: TypeId| clause_param(u, part, (binder, names), method);
    let mut used = vec![false; args.len()];
    u.map_type(t, &mut |u, part| {
        if let Some(i) = param(u, part).filter(|&i| i < args.len()) {
            used[i] = true;
        }
        None
    });
    let mut replacements = args.to_vec();
    for (r, _) in replacements.iter_mut().zip(used).filter(|&(_, used)| used) {
        if !is_stable_path(u, *r) {
            let widened = u.widen(*r);
            *r = u.fresh_skolem(widened);
        }
    }
    substituted(u, t, &param, &replacements)
}

/// The number of the parameter of the clause of binder `binder` and parameter names `names` of
/// method `method` that `part` refers to.
pub fn clause_param(
    u: &Universe,
    part: TypeId,
    (binder, names): (Binder, &[NameId]),
    method: Option<SymbolId>,
) -> Option<usize> {
    match *u.ty(part) {
        Type::Param(b, i) if b == binder => Some(i as usize),
        Type::Sym(None, p) if u.symbol(p).kind == Kind::Param && u.symbol(p).owner == method => {
            names.iter().position(|&n| u.name(n) == u.symbol(p).name)
        }
        _ => None,
    }
}

/// Whether `t` refers to a parameter of the clause of binder `binder` and parameter names `names`
/// of method `method`.
pub fn mentions_clause_params(
    u: &mut Universe,
    t: TypeId,
    clause: (Binder, &[NameId]),
    method: Option<SymbolId>,
) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= clause_param(u, part, clause, method).is_some();
        None
    });
    found
}

/// Whether `t` selects a type member on a parameter of the clause of binder `binder` and
/// parameter names `names` of method `method` whose type `formals` gives an alias for it not
/// referring to that type (a false dependency of dotty's `TermLambda.resultType`).
pub fn has_false_dependency(
    u: &mut Universe,
    t: TypeId,
    clause: (Binder, &[NameId]),
    method: Option<SymbolId>,
    formals: &[TypeId],
) -> bool {
    let mut selections = Vec::new();
    u.map_type(t, &mut |u, part| {
        let (pre, name) = match *u.ty(part) {
            Type::TypeRef(pre, name) => (pre, name),
            Type::Sym(Some(pre), s) if u.symbol(s).kind == Kind::Type => (pre, u.interned_name(s)),
            _ => return None,
        };
        selections.extend(clause_param(u, pre, clause, method).and_then(|i| Some((*formals.get(i)?, name))));
        None
    });
    selections.into_iter().any(|(formal, name)| {
        let Some(info) = u.refinement_named(formal, name) else { return false };
        let Some(alias) = u.type_alias(info) else { return false };
        let mut recursive = false;
        u.map_type(alias, &mut |_, part| {
            recursive |= part == formal;
            None
        });
        !recursive
    })
}

/// `t` with each parameter `param` numbers replaced by its replacement, a type member of a
/// replaced prefix selected again as a member of the new prefix.
pub fn substituted(
    u: &mut Universe,
    t: TypeId,
    param: &dyn Fn(&Universe, TypeId) -> Option<usize>,
    replacements: &[TypeId],
) -> TypeId {
    u.map_type(t, &mut |u, part| {
        if let Some(i) = param(u, part) {
            return replacements.get(i).copied();
        }
        if let Some((pre, name)) = u.type_ref(part) {
            let new_pre = substituted(u, pre, param, replacements);
            if new_pre == pre || !matches!(u.ty(new_pre), Type::Skolem(..) | Type::Refined(..)) {
                return None;
            }
            let alias = u.refinement_named(new_pre, name).and_then(|i| u.type_alias(i));
            return Some(alias.unwrap_or_else(|| u.intern_type(Type::TypeRef(new_pre, name))));
        }
        let Some((Some(pre), s)) = u.sym_ref(part) else { return None };
        if u.symbol(s).kind.is_term() {
            let new_pre = substituted(u, pre, param, replacements);
            let selected = u.intern_type(Type::Sym(Some(new_pre), s));
            return (new_pre != pre).then(|| refined_path(u, selected).unwrap_or(selected));
        }
        if u.symbol(s).kind != Kind::Type {
            return None;
        }
        let new_pre = substituted(u, pre, param, replacements);
        if new_pre == pre {
            return None;
        }
        let new_pre = refined_path(u, new_pre).unwrap_or(new_pre);
        if matches!(u.ty(new_pre), Type::Skolem(..) | Type::Refined(..))
            && let Some(info) = u.refinement_info(new_pre, s)
            && let Some(alias) = u.type_alias(info)
        {
            return Some(alias);
        }
        let name = u.symbol(s).name.clone();
        let members = u.member_of_type(new_pre, &name);
        let member = members.into_iter().find(|&m| u.symbol(m).kind == Kind::Type).unwrap_or(s);
        Some(u.intern_type(Type::Sym(Some(new_pre), member)))
    })
}

/// The path a refinement of skolem `s`'s type gives term member `m` for selection `s.m`.
fn refined_path(u: &mut Universe, t: TypeId) -> Option<TypeId> {
    let Some((Some(pre), m)) = u.sym_ref(t) else { return None };
    if !matches!(u.ty(pre), Type::Skolem(..)) || !u.symbol(m).kind.is_term() {
        return None;
    }
    let name = u.interned_name(m);
    let info = u.term_refinement_named(pre, name)?;
    match *u.ty(info) {
        Type::Sym(_, p) if u.symbol(p).kind.is_term() => Some(info),
        Type::TermRef(..) => Some(info),
        _ => None,
    }
}

/// Whether type `t` refers to a value parameter of the method type of binder `binder`, or of
/// method `method`.
pub fn mentions_params(u: &mut Universe, t: TypeId, binder: Binder, method: Option<SymbolId>) -> bool {
    mentions_params_except(u, t, binder, method, &[])
}

/// `mentions_params` without the parameters `applied`, those of the clauses an application
/// passed arguments for: a reference to one is such an argument (dotty's `instantiate`).
pub fn mentions_params_except(
    u: &mut Universe,
    t: TypeId,
    binder: Binder,
    method: Option<SymbolId>,
    applied: &[SymbolId],
) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        match *u.ty(part) {
            Type::Param(b, _) if b == binder => found = true,
            Type::Sym(_, s)
                if u.symbol(s).kind == Kind::Param && method.is_some() && !applied.contains(&s) =>
            {
                found |= u.symbol(s).owner == method;
            }
            _ => {}
        }
        None
    });
    found
}
