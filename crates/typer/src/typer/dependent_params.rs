//! Parameter and result types an application leaves to dotty: by-name, dependent.

use super::Typer;
use super::stability::is_stable_path;
use tasty::{Binder, Kind, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Whether parameter type `p` is by-name or refers to a parameter.
    pub fn is_special_param(&mut self, p: TypeId) -> bool {
        let mut special = false;
        self.u.map_type(p, &mut |u, part| {
            special |= matches!(u.types().get(part), Type::ByName(_) | Type::Param(..));
            None
        });
        special
    }

    pub fn applied_result(
        &mut self,
        t: TypeId,
        params: (Binder, &[String]),
        method: Option<SymbolId>,
        args: &[TypeId],
    ) -> TypeId {
        applied_result(self.u, t, params, method, args)
    }

    pub fn mentions_params(&mut self, t: TypeId, binder: Binder, method: Option<SymbolId>) -> bool {
        mentions_params(self.u, t, binder, method)
    }
}

/// Result `t` of the method type of binder `binder` and parameters `names`, of method `method`,
/// applied to arguments of types `args`: its references to the parameters replaced by the
/// arguments' types, a skolem of the widened type for an argument that is not stable (dotty's
/// `safeSubstParams`).
pub fn applied_result(
    u: &mut Universe,
    t: TypeId,
    (binder, names): (Binder, &[String]),
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
fn clause_param(
    u: &Universe,
    part: TypeId,
    (binder, names): (Binder, &[String]),
    method: Option<SymbolId>,
) -> Option<usize> {
    match *u.types().get(part) {
        Type::Param(b, i) if b == binder => Some(i as usize),
        Type::Sym(None, p) if u.symbol(p).kind == Kind::Param && u.symbol(p).owner == method => {
            names.iter().position(|n| *n == u.symbol(p).name)
        }
        _ => None,
    }
}

/// Whether `t` refers to a parameter of the clause of binder `binder` and parameter names `names`
/// of method `method`.
pub fn mentions_clause_params(
    u: &mut Universe,
    t: TypeId,
    clause: (Binder, &[String]),
    method: Option<SymbolId>,
) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= clause_param(u, part, clause, method).is_some();
        None
    });
    found
}

/// `t` with each parameter `param` numbers replaced by its replacement, a type member of a
/// replaced prefix selected again as a member of the new prefix.
fn substituted(
    u: &mut Universe,
    t: TypeId,
    param: &dyn Fn(&Universe, TypeId) -> Option<usize>,
    replacements: &[TypeId],
) -> TypeId {
    u.map_type(t, &mut |u, part| {
        if let Some(i) = param(u, part) {
            return replacements.get(i).copied();
        }
        let Type::Sym(Some(pre), s) = *u.types().get(part) else { return None };
        if u.symbol(s).kind != Kind::Type {
            return None;
        }
        let new_pre = substituted(u, pre, param, replacements);
        if new_pre == pre {
            return None;
        }
        let name = u.symbol(s).name.clone();
        if let Some(alias) = refined_alias(u, new_pre, &name) {
            return Some(alias);
        }
        let members = u.member_of_type(new_pre, &name);
        let member = members.into_iter().find(|&m| u.symbol(m).kind == Kind::Type).unwrap_or(s);
        Some(u.intern_type(Type::Sym(Some(new_pre), member)))
    })
}

/// The alias a refinement of `pre` gives type member `name` (dotty's `lookupRefined`).
fn refined_alias(u: &mut Universe, mut pre: TypeId, name: &str) -> Option<TypeId> {
    for _ in 0..64 {
        pre = match u.types().get(pre).clone() {
            Type::Refined(_, n, info) if n == name => {
                return match *u.types().get(info) {
                    Type::Alias(a) => Some(a),
                    Type::Bounds(..) | Type::Wildcard(_) => None,
                    _ => Some(info),
                };
            }
            Type::Refined(p, ..) | Type::Skolem(_, p) | Type::Annotated(p, _) => p,
            Type::Sym(_, s) if u.symbol(s).kind == Kind::Type => u.alias_of(s)?,
            _ => return None,
        };
    }
    None
}

/// Whether type `t` refers to a value parameter of the method type of binder `binder`, or of
/// method `method`.
pub fn mentions_params(u: &mut Universe, t: TypeId, binder: Binder, method: Option<SymbolId>) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        match *u.types().get(part) {
            Type::Param(b, _) if b == binder => found = true,
            Type::Sym(_, s) if u.symbol(s).kind == Kind::Param && method.is_some() => {
                found |= u.symbol(s).owner == method;
            }
            _ => {}
        }
        None
    });
    found
}
