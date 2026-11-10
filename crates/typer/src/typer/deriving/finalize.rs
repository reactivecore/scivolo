// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Deriving.scala
//! Derived instances typed after the statements around their companion (dotty's
//! `Deriver.syntheticDefs`).

use super::super::Typer;
use super::Problem;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::NodeId;
use tasty::SymbolId;

/// The instances of module class `cls`'s `derives` clause, typed in `ctx`; one whose type
/// class's `derived` cannot be called reported.
pub fn typed_instances(t: &mut Typer<'_>, cls: SymbolId, ctx: Ctx) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let instances = t.derivations.instances.remove(&cls).unwrap_or_default();
    let defs: Vec<NodeId> = instances.iter().map(|i| i.def).collect();
    for instance in instances {
        if let Some(problem) = instance.problem {
            report(t, instance.def, problem);
            continue;
        }
        let typed = t.typed_stat(instance.def, ctx);
        out.push(with_plain_type_trees(&mut t.tpd, typed));
    }
    t.derivations.lambdas.retain(|_, def| !defs.contains(def));
    out
}

/// Reports why the `TC.derived` of the instance defined by `d` cannot be called.
fn report(t: &mut Typer<'_>, d: NodeId, problem: Problem) {
    let sym = t.syms.by_tree[&d];
    t.complete(sym);
    let result = match t.syms.def_results.get(&sym) {
        Some(&r) => r,
        None => t.u.info(sym),
    };
    let shown = t.show_type(result);
    let msg = match problem {
        Problem::NoCompanion(tc) => format!("{shown} cannot be derived since {tc} has no companion object"),
        Problem::ExplicitParams(module) => format!(
            "derived instance {shown} failed to generate:\nmethod `derived` from object {module} takes explicit term parameters"
        ),
    };
    t.error_at(t.ast.span(d), msg);
}

/// Typed instance definition `def` with plain type trees for its parameters' and result's
/// types, as dotty's `tpd.DefDef` of a symbol has.
fn with_plain_type_trees(tpd: &mut Tpd, def: TNodeId) -> TNodeId {
    let plain = |tpd: &mut Tpd, n: TNodeId| tpd.get_mut(n).tree = T::TypeTree { inferred: false };
    match *tpd.tree(def) {
        T::ValDef { tpt, .. } => plain(tpd, tpt),
        T::DefDef { paramss, tpt, .. } => {
            let params: Vec<TNodeId> =
                tpd.clause_items(paramss).iter().flat_map(|&c| tpd.items(c).to_vec()).collect();
            for p in params {
                match *tpd.tree(p) {
                    T::ValDef { tpt, .. } => plain(tpd, tpt),
                    T::TypeDef { rhs, .. } => plain(tpd, rhs),
                    _ => {}
                }
            }
            plain(tpd, tpt);
        }
        _ => {}
    }
    def
}
