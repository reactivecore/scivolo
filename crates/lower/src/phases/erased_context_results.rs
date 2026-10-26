// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ContextFunctionResults.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! Context function results with erased parameters: `PolyFunction { def apply(using …): R }`,
//! and the `$direct` name of a flattened method whose context parameters are all erased.

use crate::Lower;
use crate::symbols::has;
use tasty::tags::PRIVATE;
use tasty::{SymbolId, Type, TypeId};

/// Whether `t` is `PolyFunction { def apply(using …): R }` (dotty's `ContextFunctionType` of a
/// `PolyFunctionOf`).
pub fn is_erased_context_function(l: &mut Lower, t: TypeId) -> bool {
    let t = l.u.dealias(t);
    let info = l.u.poly_function_apply(t);
    info.is_some_and(|i| matches!(l.u.types().get(i), Type::Method { contextual: true, .. }))
}

/// Names flattened method `m` of type `info` before flattening `<name>$direct` if all
/// parameters of the context functions of its result are erased and it is a trait's or
/// overrides a member (dotty's `contextResultsAreErased`, `Erasure`'s `erasedName`); `m` is in
/// `l.context_results` already.
pub fn rename_direct(l: &mut Lower, m: SymbolId, info: TypeId) {
    let mut t = info;
    while let Type::Poly { result, .. } | Type::Method { result, .. } = *l.u.types().get(t) {
        t = result;
    }
    if !all_erased(l, t) {
        return;
    }
    let Some(owner) = l.u.symbol(m).owner else { return };
    let name = l.u.symbol(m).name.clone();
    let site = l.u.this_type(owner);
    let overrides = l.u.linearization(owner).into_iter().skip(1).any(|b| {
        let decls = l.u.decl(b, &name).unwrap_or_default();
        decls.into_iter().any(|d| {
            l.u.symbol(d).kind.is_term() && !has(l.u, d, PRIVATE) && super::bridges::matching(l, site, m, d)
        })
    });
    if l.u.is_trait(owner) || overrides {
        let target = l.u.symbol(m).target.clone().unwrap_or(name);
        l.u.set_target(m, &format!("{target}$direct"));
    }
}

/// Whether the context functions `t` nests take erased parameters only.
fn all_erased(l: &mut Lower, t: TypeId) -> bool {
    let t = l.u.dealias(t);
    if let Type::Applied(..) = l.u.types().get(t)
        && super::context_results::is_context_function(l, t)
    {
        return false;
    }
    let Some(info) = l.u.poly_function_apply(t) else { return true };
    let Type::Method { params, result, contextual: true, .. } = l.u.types().get(info).clone() else {
        return true;
    };
    params.into_iter().all(|(_, p)| l.u.is_erased_param(p)) && all_erased(l, result)
}
