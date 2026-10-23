// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ContextFunctionResults.scala
//! Library methods annotated `ContextResultCount`: their context function results as parameter
//! clauses (dotty's `integrateContextResults`), as many as their result types nest.

use super::context_results::{is_annotated, is_context_function, synthetic_names, with_result};
use crate::Lower;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// For reference `t` to annotated library method `m` not yet flattened, flattens its type,
/// recording its type before; returns how many clauses it had and how many closures it takes.
pub fn flatten_reference(l: &mut Lower, t: TNodeId, m: SymbolId) -> Option<(usize, usize)> {
    let reference = matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. });
    if !reference || l.context_results.contains_key(&m) || !is_annotated(l, m) {
        return None;
    }
    let info = l.u.info(m);
    let clauses = super::context_bridges::clauses(l, info).len();
    let mut result = l.u.result_type(info);
    let mut methods = Vec::new();
    while is_context_function(l, result) {
        let function = l.u.dealias(result);
        let Type::Applied(_, args) = l.u.types().get(function).clone() else { break };
        let Some((&last, params)) = args.split_last() else { break };
        let binder = l.u.fresh_binder();
        let params = params.iter().map(|&p| (String::new(), p)).collect();
        let method = Type::Method { binder, params, result: last, implicit: false, contextual: true };
        let method = l.u.intern_type(method);
        methods.push(synthetic_names(l, method));
        result = last;
    }
    let depth = methods.len();
    let inner = methods.into_iter().rev().reduce(|inner, outer| with_result(l, outer, inner))?;
    let flat = with_result(l, info, inner);
    l.u.set_info(m, flat);
    l.context_results.insert(m, (info, depth));
    Some((clauses, depth))
}
