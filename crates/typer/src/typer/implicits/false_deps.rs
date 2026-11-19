// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The result of a method type with its false dependencies dealiased (dotty's
//! `TermLambda.resultType` for `FalseDeps`).

use super::super::dependent_params::{clause_param, mentions_clause_params};
use super::super::type_ops::TypeOps;
use tasty::{Binder, Kind, NameId, SymbolId, Type, TypeId, Universe};

impl TypeOps<'_> {
    /// Dealiases the result `t` of a clause of binder `binder` and parameters `params` of method
    /// `meth` where it selects an alias member of a parameter, as `(using c: C { type T = Int })`
    /// with result `c.T` gives `Int`; `t` itself if a parameter is still referred to then.
    pub fn without_false_deps_in(
        &mut self,
        t: TypeId,
        clause: (Binder, &[(NameId, TypeId)]),
        meth: Option<SymbolId>,
    ) -> TypeId {
        without_false_deps(self.u, t, clause, meth)
    }
}

/// Determines `TypeOps::without_false_deps_in` of `t`.
pub fn without_false_deps(
    u: &mut Universe,
    t: TypeId,
    (binder, params): (Binder, &[(NameId, TypeId)]),
    meth: Option<SymbolId>,
) -> TypeId {
    let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
    if !mentions_clause_params(u, t, (binder, &names), meth) {
        return t;
    }
    let dealiased = u.map_type(t, &mut |u, part| {
        let Some((Some(pre), s)) = u.sym_ref(part) else { return None };
        if u.symbol(s).kind != Kind::Type {
            return None;
        }
        let i = clause_param(u, pre, (binder, &names), meth)?;
        let info = u.member_info(params.get(i)?.1, s);
        u.type_alias(info)
    });
    match mentions_clause_params(u, dealiased, (binder, &names), meth) {
        true => t,
        false => dealiased,
    }
}

/// Dealiases the false dependencies of the results of the method types `t` starts with, as
/// mapping a method type over its `resultType` does in dotty.
pub fn method_without_false_deps(u: &mut Universe, t: TypeId, meth: Option<SymbolId>) -> TypeId {
    let Type::Method { binder, params, result, implicit, contextual } = *u.ty(t) else { return t };
    let clause = u.params_of(params).to_vec();
    let inner = method_without_false_deps(u, result, meth);
    let dealiased = without_false_deps(u, inner, (binder, &clause), meth);
    if dealiased == result {
        return t;
    }
    u.intern_type(Type::Method { binder, params, result: dealiased, implicit, contextual })
}
