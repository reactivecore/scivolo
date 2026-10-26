// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The definitions erasure drops (dotty's `isEffectivelyErased` and `erasedDef`): inline methods
//! and values, erased values and parameters; and the arguments of erased parameters
//! (`Erasure.Typer.typedApply`), which must be pure (`checkPureErased`).

use super::super::elideable::is_elideable;
use super::super::walk::subtrees;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::SymbolId;
use tasty::tags::{ERASED, INLINE};
use typer::tpd::{TNodeId, Tree};

/// Drops the inline and erased definitions of the class bodies and blocks in unit `root`, the
/// erased parameters of its methods and the arguments of erased parameters, `inits` the unit's
/// objects with initialization code; the definitions dropped.
pub fn drop(l: &mut Lower, root: TNodeId, inits: &FxHashSet<SymbolId>) -> Result<Vec<TNodeId>> {
    let mut dropped = Vec::new();
    for t in subtrees(l.tpd, root) {
        match *l.tpd.tree(t) {
            Tree::Block { stats: list, .. } | Tree::Template { body: list, .. } => {
                let (erased, kept): (Vec<TNodeId>, Vec<TNodeId>) =
                    l.tpd.items(list).iter().partition(|&&s| is_effectively_erased(l, s));
                if erased.is_empty() {
                    continue;
                }
                dropped.extend(erased);
                let kept = l.tpd.list(&kept);
                match &mut l.tpd.get_mut(t).tree {
                    Tree::Block { stats, .. } => *stats = kept,
                    Tree::Template { body, .. } => *body = kept,
                    _ => unreachable!("a block or template"),
                }
            }
            Tree::DefDef { paramss, .. } => {
                let clauses = l.tpd.clause_items(paramss).to_vec();
                if !clauses.iter().any(|&c| l.tpd.items(c).iter().any(|&p| is_erased(l, p))) {
                    continue;
                }
                let kept: Vec<_> = clauses
                    .into_iter()
                    .map(|c| {
                        let ps: Vec<TNodeId> =
                            l.tpd.items(c).iter().copied().filter(|&p| !is_erased(l, p)).collect();
                        l.tpd.list(&ps)
                    })
                    .collect();
                let kept = l.tpd.clauses(&kept);
                if let Tree::DefDef { paramss, .. } = &mut l.tpd.get_mut(t).tree {
                    *paramss = kept;
                }
            }
            Tree::Apply { fun, args } => drop_erased_args(l, t, (fun, args), inits)?,
            _ => {}
        }
    }
    refuse_erased_refs(l, root)?;
    Ok(dropped)
}

/// Drops the arguments of the erased parameters of call `t`, refusing impure ones.
fn drop_erased_args(
    l: &mut Lower,
    t: TNodeId,
    (fun, args): (TNodeId, typer::tpd::List),
    inits: &FxHashSet<SymbolId>,
) -> Result<()> {
    let method = l.u.widen(l.tpd.tpe(fun));
    let tasty::Type::Method { params, .. } = l.u.types().get(method).clone() else { return Ok(()) };
    let erased: Vec<bool> = params.iter().map(|&(_, p)| l.u.is_erased_param(p)).collect();
    if !erased.contains(&true) {
        return Ok(());
    }
    let mut kept = Vec::new();
    for (i, a) in l.tpd.items(args).to_vec().into_iter().enumerate() {
        if !erased.get(i).copied().unwrap_or(false) {
            kept.push(a);
        } else if !is_elideable(l, a, inits) {
            return unsupported(l.tpd.span(a), "argument of an erased parameter that is not pure");
        }
    }
    let args = l.tpd.list(&kept);
    l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    Ok(())
}

/// Refuses references to erased values left in unit `root` (dotty's `checkNotErased`).
fn refuse_erased_refs(l: &mut Lower, root: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, root) {
        if matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. })
            && l.tpd.sym(t).is_some_and(|s| is_erased_sym(l, s))
        {
            return unsupported(l.tpd.span(t), "reference to an erased value");
        }
    }
    Ok(())
}

/// Whether term `s` is erased: flagged so, or `erasedValue` of `scala.compiletime` or
/// `unsafeErasedValue` of `scala.caps` (dotty's `isErased`).
fn is_erased_sym(l: &Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    if !sym.kind.is_term() {
        return false;
    }
    let full = l.u.full_name(s);
    has(l.u, s, ERASED)
        || matches!(sym.name.as_str(), "erasedValue" | "unsafeErasedValue")
            && (full.starts_with("scala.compiletime.") || full.starts_with("scala.caps."))
}

/// Whether `stat` defines an inline or erased method or value (an erased value's getter too).
fn is_effectively_erased(l: &Lower, stat: TNodeId) -> bool {
    matches!(l.tpd.tree(stat), Tree::DefDef { .. } | Tree::ValDef { .. })
        && l.tpd.sym(stat).is_some_and(|s| has(l.u, s, INLINE) || has(l.u, s, ERASED))
}

/// Whether `stat` defines an erased value or parameter.
fn is_erased(l: &Lower, stat: TNodeId) -> bool {
    matches!(l.tpd.tree(stat), Tree::ValDef { .. }) && l.tpd.sym(stat).is_some_and(|s| has(l.u, s, ERASED))
}
