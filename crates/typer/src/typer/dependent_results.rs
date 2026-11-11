// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Parameter and result types of applications of dependent methods (dotty's
//! `TermLambda.resultType`, `safeSubstParam` in `matchArgs` and `safeSubstParams` in `assignType`).

use super::Typer;
use super::dependent_params::{
    applied_result, has_false_dependency, mentions_clause_params, mentions_params, mentions_params_except,
};
use crate::tpd::TNodeId;
use tasty::{Binder, Kind, NameId, SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether parameter type `p` is by-name or refers to a parameter it does not bind.
    pub fn is_special_param(&mut self, p: TypeId) -> bool {
        let mut special = self.u.by_name(p).is_some();
        let mut bound = Vec::new();
        self.u.map_type(p, &mut |u, part| {
            if let Some(b) = u.poly(part).or_else(|| u.method(part)).or_else(|| u.lambda(part)) {
                bound.push(b.binder);
            }
            None
        });
        self.u.map_type(p, &mut |u, part| {
            special |= u.param_ref(part).is_some_and(|(b, _)| !bound.contains(&b));
            None
        });
        special
    }

    pub fn applied_result(
        &mut self,
        t: TypeId,
        params: (Binder, &[NameId]),
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
        clause: (Binder, &[NameId]),
        method: Option<SymbolId>,
    ) -> bool {
        mentions_clause_params(self.u, t, clause, method)
    }

    /// Whether one of `formals` of the clause `clause` of method `method` refers to an earlier
    /// parameter of the clause; `Err` for a formal outside the subset: by-name inside, referring
    /// to a parameter of another clause but those `applied`, or to its own clause unless the
    /// arguments are `in_order`.
    pub fn param_dependent(
        &mut self,
        formals: &[TypeId],
        (clause, method): ((Binder, &[NameId]), Option<SymbolId>),
        applied: &[SymbolId],
        in_order: bool,
    ) -> Result<bool, &'static str> {
        let mut dependent = false;
        for &p in formals {
            let own = self.mentions_clause_params(p, clause, method);
            dependent |= own;
            if own && !in_order
                || !own
                    && (self.is_special_param(p)
                        || mentions_params_except(self.u, p, clause.0, method, applied))
            {
                return Err("dependent parameter");
            }
        }
        Ok(dependent)
    }

    /// Formal `formal` of the clause `clause` of method `method` with the types of the arguments
    /// `before` typed so far substituted for its parameters.
    pub fn dependent_formal(
        &mut self,
        formal: TypeId,
        (clause, method): ((Binder, &[NameId]), Option<SymbolId>),
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
        let (binder, result) = self.u.method(info).map(|m| (m.binder, m.result))?;
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
                let Some((Some(pre), s)) = u.sym_ref(part) else { return None };
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
    /// there is one argument per parameter and `t` has no false dependency on them.
    pub fn dependent_result(
        &mut self,
        t: TypeId,
        (binder, params): (Binder, &[(NameId, TypeId)]),
        method: Option<SymbolId>,
        args: &[TNodeId],
    ) -> Option<TypeId> {
        let types: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
        let formals: Vec<TypeId> = params.iter().map(|&(_, p)| self.zonked_upper(p)).collect();
        if types.len() != params.len() || has_false_dependency(self.u, t, (binder, &names), method, &formals)
        {
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

    /// `t` zonked, an uninstantiated type variable as its upper bound, a type member a
    /// refinement of its prefix aliases as the alias.
    fn zonked_upper(&mut self, t: TypeId) -> TypeId {
        let mut t = self.zonk(t);
        if let Some(v) = self.u.var(t) {
            let (_, hi) = self.constraint.bounds(v);
            t = self.zonk(hi);
        }
        self.u.refinement_dealiased(t)
    }

    /// Completes the type members `t` selects.
    pub fn complete_type_members(&mut self, t: TypeId) {
        let mut members = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Some((Some(_), s)) = u.sym_ref(part)
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
        found |= matches!(u.ty(part), Type::Bounds(..) | Type::Wildcard(_));
        None
    });
    found
}
