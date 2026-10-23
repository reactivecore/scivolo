// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ContextFunctionResults.scala
//! Library methods annotated `ContextResultCount(n)`: the first `n` context function results as
//! parameter clauses (dotty's `integrateContextResults`).

use super::context_results::{is_context_function, synthetic_names, with_result};
use crate::Lower;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// For reference `t` to annotated library method `m`, flattens its type unless done before,
/// recording its type before; returns how many clauses it had and how many closures it takes.
pub fn flatten_reference(l: &mut Lower, t: TNodeId, m: SymbolId) -> Option<(usize, usize)> {
    let reference = matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. });
    if !reference || l.u.symbol(m).tree.is_none() {
        return None;
    }
    if let Some(&(before, d)) = l.context_results.get(&m) {
        return Some((super::context_bridges::clauses(l, before).len(), d));
    }
    let count = l.u.annotation_int(m, "scala.annotation.internal.ContextResultCount")?;
    let depth = usize::try_from(count).ok().filter(|&d| d > 0)?;
    let info = l.u.info(m);
    let clauses = super::context_bridges::clauses(l, info).len();
    let mut result = l.u.result_type(info);
    let mut methods = Vec::new();
    for _ in 0..depth {
        if !is_context_function(l, result) {
            return None;
        }
        let function = l.u.dealias(result);
        let Type::Applied(_, args) = l.u.types().get(function).clone() else { return None };
        let (&last, params) = args.split_last()?;
        let binder = l.u.fresh_binder();
        let params = params.iter().map(|&p| (String::new(), p)).collect();
        let method = Type::Method { binder, params, result: last, implicit: false, contextual: true };
        let method = l.u.intern_type(method);
        methods.push(synthetic_names(l, method));
        result = last;
    }
    let inner = methods.into_iter().rev().reduce(|inner, outer| with_result(l, outer, inner))?;
    let flat = with_result(l, info, inner);
    l.u.set_info(m, flat);
    l.context_results.insert(m, (info, depth));
    Some((clauses, depth))
}
