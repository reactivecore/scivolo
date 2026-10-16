//! Result types of applications of dependent methods (dotty's `TermLambda.resultType` and
//! `safeSubstParams` in `assignType`).

use super::Typer;
use super::dependent_params::{applied_result, mentions_clause_params, mentions_params};
use crate::tpd::TNodeId;
use tasty::{Binder, Kind, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Whether `t` refers to a parameter of the clause of binder `binder` and parameter names
    /// `names` of method `method`.
    pub fn mentions_clause_params(
        &mut self,
        t: TypeId,
        clause: (Binder, &[String]),
        method: Option<SymbolId>,
    ) -> bool {
        mentions_clause_params(self.u, t, clause, method)
    }

    /// Result `t` of the method type of binder `binder` of method `method` with the aliases
    /// selected from parameters dealiased, if that leaves no reference to a parameter, else `t`
    /// (dotty's `TermLambda.resultType` for `FalseDeps`).
    pub fn without_false_deps(&mut self, t: TypeId, binder: Binder, method: Option<SymbolId>) -> TypeId {
        if !mentions_params(self.u, t, binder, method) {
            return t;
        }
        let mut current = t;
        for _ in 0..8 {
            self.complete_type_members(current);
            let next = self.u.map_type(current, &mut |u, part| {
                let Type::Sym(Some(pre), s) = *u.types().get(part) else { return None };
                if u.symbol(s).kind != Kind::Type || !mentions_params(u, pre, binder, method) {
                    return None;
                }
                let alias = u.alias_of(s)?;
                let owner = u.symbol(s).owner?;
                let seen = u.as_seen_from(alias, pre, owner);
                (!has_wildcard(u, seen) || has_wildcard(u, alias)).then_some(seen)
            });
            if next == current {
                break;
            }
            current = next;
        }
        if mentions_params(self.u, current, binder, method) { t } else { current }
    }

    /// Result `t` of a method of parameters `params` applied to the typed arguments `args`, if
    /// there is one argument per parameter and no class is selected from a parameter.
    pub fn dependent_result(
        &mut self,
        t: TypeId,
        (binder, params): (Binder, &[(String, TypeId)]),
        method: Option<SymbolId>,
        args: &[TNodeId],
    ) -> Option<TypeId> {
        let types: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
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
            if mentions_clause_params(u, pre, (binder, &names), method) {
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
        let result = applied_result(self.u, t, (binder, &names), method, &types);
        self.complete_type_members(result);
        Some(result)
    }
}

/// Whether `t` has a wildcard or bounds in it.
fn has_wildcard(u: &mut Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= matches!(u.types().get(part), Type::Wildcard(_) | Type::Bounds(..));
        None
    });
    found
}

impl Typer<'_> {
    /// Completes the type members `t` selects.
    pub fn complete_type_members(&mut self, t: TypeId) {
        let mut members = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(Some(_), s) = *u.types().get(part)
                && u.symbol(s).kind == Kind::Type
            {
                members.push(s);
            }
            None
        });
        members.into_iter().for_each(|s| self.complete(s));
    }
}
