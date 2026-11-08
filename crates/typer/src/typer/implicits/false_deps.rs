// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The result of an implicit clause with its false dependencies dealiased (dotty's
//! `TermLambda.resultType` for `FalseDeps`).

use super::super::Typer;
use super::super::dependent_params::{clause_param, mentions_clause_params};
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Result `t` of a clause of binder `binder` and parameters `params` of method `meth`, each
    /// alias member selected from a parameter replaced by its alias in the parameter's type, if
    /// that leaves no reference to a parameter, else `t`.
    pub fn without_false_deps_in(
        &mut self,
        t: TypeId,
        (binder, params): (Binder, &[(String, TypeId)]),
        meth: Option<SymbolId>,
    ) -> TypeId {
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        if !mentions_clause_params(self.u, t, (binder, &names), meth) {
            return t;
        }
        let dealiased = self.u.map_type(t, &mut |u, part| {
            let Type::Sym(Some(pre), s) = *u.types().get(part) else { return None };
            if u.symbol(s).kind != Kind::Type {
                return None;
            }
            let i = clause_param(u, pre, (binder, &names), meth)?;
            let info = u.member_info(params.get(i)?.1, s);
            match *u.types().get(info) {
                Type::Alias(alias) => Some(alias),
                _ => None,
            }
        });
        match mentions_clause_params(self.u, dealiased, (binder, &names), meth) {
            true => t,
            false => dealiased,
        }
    }
}
