//! Parameter and result types of applications of dependent methods (dotty's
//! `TermLambda.resultType`, `safeSubstParam` in `matchArgs` and `safeSubstParams` in `assignType`).

use super::Typer;
use super::dependent_params::{applied_result, mentions_clause_params, mentions_params};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

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

    /// Whether one of `formals` of the clause `clause` of method `method` refers to an earlier
    /// parameter of the clause; `Err` for a formal outside the subset: by-name inside, referring
    /// to a parameter of another clause, to its own clause unless the arguments are `in_order`,
    /// or to a private type of another class.
    pub fn param_dependent(
        &mut self,
        formals: &[TypeId],
        (clause, method): ((Binder, &[String]), Option<SymbolId>),
        in_order: bool,
        ctx: Ctx,
    ) -> Result<bool, &'static str> {
        let mut dependent = false;
        for &p in formals {
            let own = self.mentions_clause_params(p, clause, method);
            dependent |= own;
            if own && !in_order
                || !own && (self.is_special_param(p) || self.mentions_params(p, clause.0, method))
                || self.mentions_foreign_private_type(p, ctx)
            {
                return Err("dependent parameter or private type");
            }
        }
        Ok(dependent)
    }

    /// Formal `formal` of the clause `clause` of method `method` with the types of the arguments
    /// `before` typed so far substituted for its parameters.
    pub fn dependent_formal(
        &mut self,
        formal: TypeId,
        (clause, method): ((Binder, &[String]), Option<SymbolId>),
        before: &[TNodeId],
    ) -> TypeId {
        let types: Vec<TypeId> = before.iter().map(|&a| self.tpd.tpe(a)).collect();
        let formal = applied_result(self.u, formal, clause, method, &types);
        self.complete_type_members(formal);
        formal
    }

    /// The result type of method `f` of method `method` after its arguments were lifted, its
    /// false dependencies dealiased; `None` if it still refers to a parameter.
    pub fn lifted_result(&mut self, f: TNodeId, method: Option<SymbolId>) -> Option<TypeId> {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Method { binder, result, .. } = *self.u.types().get(info) else { return None };
        let result = self.without_false_deps(result, binder, method);
        (!self.mentions_params(result, binder, method)).then_some(result)
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
                let seen = u.referenced_alias(part)?;
                let declared = u.alias_of(s).is_some_and(|a| has_wildcard(u, a));
                (declared || !has_wildcard(u, seen)).then_some(seen)
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

    /// Completes the type members `t` selects.
    fn complete_type_members(&mut self, t: TypeId) {
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

/// Whether `t` has a wildcard type argument or is one.
pub fn has_wildcard(u: &mut tasty::Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= matches!(u.types().get(part), Type::Bounds(..) | Type::Wildcard(_));
        None
    });
    found
}
