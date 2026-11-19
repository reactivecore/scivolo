// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The result of a method type with the aliases selected from its parameters dealiased (dotty's
//! `TermLambda.resultType` for `FalseDeps`).

use super::Typer;
use super::dependent_params::{clause_param, mentions_clause_params};
use super::dependent_results::has_wildcard;
use tasty::{Binder, Kind, NameId, SymbolId, Type, TypeId, Universe};

/// How many rounds of dealiasing `without_false_deps` takes at most.
const MAX_DEALIAS_ROUNDS: usize = 8;

impl Typer<'_> {
    /// Result `t` of the method type of binder `binder` and parameters `params` of method `method`
    /// with the aliases selected from parameters dealiased, if that leaves no reference to a
    /// parameter, else `t` (dotty's `TermLambda.resultType` for `FalseDeps`). An alias selected
    /// from a parameter of the clause is seen from the parameter's type in the method type, as
    /// `y.TT` of `def foo[U](y: C[U])(z: y.TT)` with `class C[T] { type TT = T }` gives the type
    /// variable for `U`.
    pub fn without_false_deps(
        &mut self,
        t: TypeId,
        (binder, params): (Binder, &[(NameId, TypeId)]),
        method: Option<SymbolId>,
    ) -> TypeId {
        let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
        let clause = (binder, &names[..]);
        if !mentions_clause_params(self.u, t, clause, method) {
            return t;
        }
        let mut current = t;
        for _ in 0..MAX_DEALIAS_ROUNDS {
            self.complete_type_members(current);
            let next = self.u.map_type(current, &mut |u, part| {
                let Some((Some(pre), s)) = u.sym_ref(part) else { return None };
                if u.symbol(s).kind != Kind::Type || !mentions_clause_params(u, pre, clause, method) {
                    return None;
                }
                let seen = match clause_param(u, pre, (binder, &names), method) {
                    Some(i) => alias_seen_from_param(u, (pre, s), params[i].1)?,
                    None => u.referenced_alias(part)?,
                };
                let declared = u.alias_of(s).is_some_and(|a| has_wildcard(u, a));
                (declared || !has_wildcard(u, seen)).then_some(seen)
            });
            if next == current {
                break;
            }
            current = next;
        }
        if mentions_clause_params(self.u, current, clause, method) { t } else { current }
    }

    /// Returns method type `t` of method `method` with the false dependencies of its results on
    /// the parameters of each clause dealiased, as `(y: C[String])(z: y.TT): Unit` becomes
    /// `(y: C[String])(z: String): Unit` (dotty's `resultType` of a method type with `FalseDeps`).
    pub fn with_false_deps_dealiased(&mut self, t: TypeId, method: Option<SymbolId>) -> TypeId {
        let Type::Method { binder, params, result, implicit, contextual } = *self.u.ty(t) else { return t };
        let params = self.u.params_of(params).to_vec();
        let dealiased = self.without_false_deps(result, (binder, &params), method);
        let dealiased = self.with_false_deps_dealiased(dealiased, method);
        match dealiased == result {
            true => t,
            false => self.u.intern_method(binder, &params, dealiased, implicit, contextual),
        }
    }
}

/// The alias that type member `s` of parameter reference `param` names, seen from a skolem of
/// the parameter's type `info` that stands for `param` (dotty's `TermParamRef.underlying`).
fn alias_seen_from_param(u: &mut Universe, (param, s): (TypeId, SymbolId), info: TypeId) -> Option<TypeId> {
    let info = u.by_name(info).unwrap_or(info);
    let skolem = u.fresh_skolem(info);
    let selected = u.intern_type(Type::Sym(Some(skolem), s));
    let alias = u.referenced_alias(selected)?;
    Some(u.map_type(alias, &mut |_, part| (part == skolem).then_some(param)))
}
