// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The result of an implicit clause with its false dependencies dealiased (dotty's
//! `TermLambda.resultType` for `FalseDeps`).

use super::super::dependent_params::{clause_param, mentions_clause_params};
use super::super::type_ops::TypeOps;
use tasty::{Binder, Kind, SymbolId, TypeId};

impl TypeOps<'_> {
    /// Dealiases the result `t` of a clause of binder `binder` and parameters `params` of method
    /// `meth` where it selects an alias member of a parameter, as `(using c: C { type T = Int })`
    /// with result `c.T` gives `Int`; `t` itself if a parameter is still referred to then.
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
            let Some((Some(pre), s)) = u.sym_ref(part) else { return None };
            if u.symbol(s).kind != Kind::Type {
                return None;
            }
            let i = clause_param(u, pre, (binder, &names), meth)?;
            let info = u.member_info(params.get(i)?.1, s);
            u.type_alias(info)
        });
        match mentions_clause_params(self.u, dealiased, (binder, &names), meth) {
            true => t,
            false => dealiased,
        }
    }
}
