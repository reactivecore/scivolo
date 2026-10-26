// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The definitions erasure drops (dotty's `isEffectivelyErased` and `erasedDef`): inline methods
//! and values, erased values and parameters; and the arguments of erased parameters
//! (`Erasure.Typer.typedApply`), which must be pure (`checkPureErased`).

use super::super::casts::is_cast;
use super::super::elideable::is_elideable;
use super::super::walk::subtrees;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{ERASED, INLINE};
use tasty::{Kind, SymbolId, Type};
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
                for &s in &erased {
                    let Tree::ValDef { rhs: Some(rhs), .. } = *l.tpd.tree(s) else { continue };
                    if is_erased(l, s) && !is_pure_arg(l, rhs, inits) {
                        return unsupported(
                            l.tpd.span(rhs),
                            "right-hand side of an erased value that is not pure",
                        );
                    }
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
    let method = l.u.poly_function_apply_of(method).unwrap_or(method);
    let tasty::Type::Method { params, .. } = l.u.types().get(method).clone() else { return Ok(()) };
    let erased: Vec<bool> = params.iter().map(|&(_, p)| l.u.is_erased_param(p)).collect();
    if !erased.contains(&true) {
        return Ok(());
    }
    let mut kept = Vec::new();
    for (i, a) in l.tpd.items(args).to_vec().into_iter().enumerate() {
        if !erased.get(i).copied().unwrap_or(false) {
            kept.push(a);
        } else if !is_pure_arg(l, a, inits) {
            return unsupported(l.tpd.span(a), "argument of an erased parameter that is not pure");
        }
    }
    let args = l.tpd.list(&kept);
    l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    Ok(())
}

/// Whether argument `t` of an erased parameter or right-hand side of an erased value is pure
/// (dotty's `isPureExpr`): elideable with pure type applications only, an erased value of
/// `scala.caps` (`capsErasedValueMethods`), or a reference to an inline value of a pure qualifier.
fn is_pure_arg(l: &mut Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let type_applies = subtrees(l.tpd, t).into_iter().all(|n| match *l.tpd.tree(n) {
        Tree::TypeApply { fun, .. } => is_pure_type_apply(l, n, fun),
        _ => true,
    });
    type_applies && is_pure_expr(l, t, inits)
}

fn is_pure_expr(l: &mut Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let inline = l.tpd.sym(t).is_some_and(|s| has(l.u, s, INLINE) && l.u.symbol(s).kind == Kind::Val);
    match *l.tpd.tree(t) {
        Tree::Ident { .. } if inline => true,
        Tree::Select { qualifier, .. } if inline => is_elideable(l, qualifier, inits),
        Tree::TypeApply { fun, .. } if l.tpd.sym(fun).is_some_and(|s| is_caps_erased_value(l, s)) => true,
        Tree::Typed { expr, .. } => is_pure_expr(l, expr, inits),
        Tree::Block { stats, expr } if l.tpd.items(stats).is_empty() => is_pure_expr(l, expr, inits),
        _ => is_elideable(l, t, inits),
    }
}

/// Whether type application `t` of `fun` is pure (dotty's `exprPurity` of a `TypeApply`): not
/// yet applied to its arguments, a cast, an erased value of `scala.caps`, or `erasedValue` of a
/// constant type.
fn is_pure_type_apply(l: &mut Lower, t: TNodeId, fun: TNodeId) -> bool {
    let tpe = l.tpd.tpe(t);
    let wide = l.u.widen(tpe);
    if matches!(l.u.types().get(wide), Type::Method { .. } | Type::Poly { .. }) || is_cast(l, t) {
        return true;
    }
    let Some(s) = l.tpd.sym(fun).filter(|&s| is_erased_sym(l, s) && !has(l.u, s, ERASED)) else {
        return false;
    };
    let tpe = l.u.dealias(tpe);
    is_caps_erased_value(l, s) || matches!(l.u.types().get(tpe), Type::Constant(..) | Type::Str(_))
}

fn is_caps_erased_value(l: &Lower, s: SymbolId) -> bool {
    is_erased_sym(l, s) && !has(l.u, s, ERASED) && l.u.full_name(s).starts_with("scala.caps.")
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
