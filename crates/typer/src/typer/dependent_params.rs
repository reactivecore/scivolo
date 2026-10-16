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

    /// Result `t` of a method of parameters `params` applied to the typed arguments `args`, if
    /// there is one argument per parameter and every one is stable.
    pub fn dependent_result(
        &mut self,
        t: TypeId,
        (binder, params): (Binder, &[(String, TypeId)]),
        method: Option<SymbolId>,
        args: &[crate::tpd::TNodeId],
    ) -> Option<TypeId> {
        let types: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        if types.len() != params.len() || !types.iter().all(|&a| is_stable_path(self.u, a)) {
            return None;
        }
        let mut inner_class = false;
        self.u.map_type(t, &mut |u, part| {
            let (pre, s) = match u.types().get(part).clone() {
                Type::Sym(Some(pre), s) => (pre, Some(s)),
                Type::TypeRef(pre, _) => (pre, u.resolve_ref(part)),
                _ => return None,
            };
            if mentions_params(u, pre, binder, method) {
                inner_class |= s.is_some_and(|s| u.symbol(s).kind == Kind::Class);
            }
            None
        });
        if inner_class {
            return None;
        }
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        Some(applied_result(self.u, t, (binder, &names), method, &types))
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
    if !mentions_params(u, t, binder, method) {
        return t;
    }
    let param = |u: &Universe, part: TypeId| match *u.types().get(part) {
        Type::Param(b, i) if b == binder => Some(i as usize),
        Type::Sym(None, p) if u.symbol(p).kind == Kind::Param && u.symbol(p).owner == method => {
            names.iter().position(|n| *n == u.symbol(p).name)
        }
        _ => None,
    };
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
    u.map_type(t, &mut |u, part| replacements.get(param(u, part)?).copied())
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
