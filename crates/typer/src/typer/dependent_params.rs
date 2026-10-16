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

    /// Result `t` of the method type of binder `binder` of method `method` with its references to
    /// the parameters dealiased if each is the prefix of an alias, else `t` (dotty's
    /// `TermLambda.resultType` for `FalseDeps`).
    pub fn without_false_deps(&mut self, t: TypeId, binder: Binder, method: Option<SymbolId>) -> TypeId {
        if !mentions_params(self.u, t, binder, method) {
            return t;
        }
        let mut current = t;
        for _ in 0..8 {
            let mut members = Vec::new();
            self.u.map_type(current, &mut |u, part| {
                if let Type::Sym(Some(_), s) = *u.types().get(part)
                    && u.symbol(s).kind == Kind::Type
                {
                    members.push(s);
                }
                None
            });
            members.into_iter().for_each(|s| self.complete(s));
            let next = self.u.map_type(current, &mut |u, part| {
                let Type::Sym(Some(pre), s) = *u.types().get(part) else { return None };
                if u.symbol(s).kind != Kind::Type || !mentions_params(u, pre, binder, method) {
                    return None;
                }
                let alias = u.alias_of(s)?;
                let owner = u.symbol(s).owner?;
                Some(u.as_seen_from(alias, pre, owner))
            });
            if !mentions_params(self.u, next, binder, method) {
                return next;
            }
            if next == current {
                return t;
            }
            current = next;
        }
        t
    }

    /// Result `t` of a method of parameters `params` applied to the typed arguments `args`, if
    /// there is one argument per parameter.
    pub fn dependent_result(
        &mut self,
        t: TypeId,
        (binder, params): (Binder, &[(String, TypeId)]),
        method: Option<SymbolId>,
        args: &[crate::tpd::TNodeId],
    ) -> Option<TypeId> {
        let types: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        if types.len() != params.len() {
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
        for &a in &types {
            let widened = self.u.widen(a);
            if let Some(c) = self.u.class_symbol(widened) {
                self.complete(c);
            }
        }
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let result = applied_result(self.u, t, (binder, &names), method, &types);
        let mut members = Vec::new();
        self.u.map_type(result, &mut |u, part| {
            if let Type::Sym(Some(_), s) = *u.types().get(part)
                && u.symbol(s).kind == Kind::Type
            {
                members.push(s);
            }
            None
        });
        members.into_iter().for_each(|s| self.complete(s));
        Some(result)
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
    substituted(u, t, &param, &replacements)
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
        let members = u.member_of_type(new_pre, &name);
        let member = members.into_iter().find(|&m| u.symbol(m).kind == Kind::Type).unwrap_or(s);
        Some(u.intern_type(Type::Sym(Some(new_pre), member)))
    })
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
