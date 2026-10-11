//! Trees whose evaluation may be dropped (dotty's `Inliner.isElideableExpr`), with the purity of
//! references and applications it needs (`TreeInfo.refPurity`, `isPureApply`).

use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use tasty::tags::{CASE, ERASED, LAZY, MUTABLE, STABLE, SYNTHETIC, TRAIT};
use tasty::{Kind, PRIMITIVE_VALUE_CLASSES, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Whether `t` is elideable, `inits` the unit's objects with initialization code.
pub fn is_elideable(l: &mut Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    match *l.tpd.tree(t) {
        Tree::This { .. } | Tree::Super { .. } | Tree::Literal { .. } | Tree::ClassLiteral { .. } => true,
        Tree::Ident { .. } => is_pure_ref(l, t, inits),
        Tree::Select { qualifier, .. } => {
            l.tpd.sym(t).is_some_and(|s| has(l.u, s, ERASED))
                || is_pure_ref(l, t, inits) && is_elideable(l, qualifier, inits)
        }
        Tree::New { .. } | Tree::Closure { .. } => true,
        Tree::TypeApply { fun, .. } => is_elideable(l, fun, inits),
        Tree::Apply { fun, args } => {
            let args = l.tpd.items(args).to_vec();
            if is_pure_apply(l, t, fun, inits) {
                is_elideable(l, fun, inits) && args.into_iter().all(|a| is_elideable(l, a, inits))
            } else if is_case_class_apply(l, t, fun) {
                args.into_iter().all(|a| is_elideable(l, a, inits))
            } else {
                callee(l, fun).is_some_and(|s| has(l.u, s, ERASED))
            }
        }
        Tree::Typed { expr, .. } | Tree::NamedArg { arg: expr, .. } => is_elideable(l, expr, inits),
        Tree::Block { stats, expr } => {
            let stats = l.tpd.items(stats).to_vec();
            is_elideable(l, expr, inits) && stats.into_iter().all(|s| is_stat_elideable(l, s, inits))
        }
        _ => false,
    }
}

fn is_stat_elideable(l: &mut Lower, s: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    match *l.tpd.tree(s) {
        Tree::TypeDef { .. } | Tree::DefDef { .. } | Tree::Import { .. } => true,
        Tree::ValDef { rhs, .. } => {
            let mutable = l.tpd.sym(s).is_some_and(|v| has(l.u, v, MUTABLE));
            !mutable && rhs.is_none_or(|r| is_elideable(l, r, inits))
        }
        _ => false,
    }
}

/// Whether reference `t` is a pure path (dotty's `refPurity == PurePath`).
fn is_pure_ref(l: &mut Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let Some(s) = l.tpd.sym(t) else { return false };
    let tpe = l.tpd.tpe(t);
    let info = l.u.widen(tpe);
    if matches!(l.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) || has(l.u, s, ERASED) {
        return true;
    }
    if matches!(l.u.types().get(tpe), Type::Constant(..) | Type::Str(_)) {
        return true;
    }
    if !is_stable_member(l, s) {
        return false;
    }
    if l.u.is_module(s) {
        return !inits.contains(&s) && l.u.module_class(s).is_some_and(|c| is_no_inits_real_class(l, c));
    }
    !has(l.u, s, LAZY)
}

/// Whether `s` is a stable member: an object, a parameter or value that is not mutable, or a
/// method marked stable.
fn is_stable_member(l: &mut Lower, s: SymbolId) -> bool {
    match l.u.symbol(s).kind {
        Kind::Param | Kind::Val => !has(l.u, s, MUTABLE),
        Kind::Def => has(l.u, s, STABLE),
        _ => l.u.is_module(s),
    }
}

fn is_no_inits_real_class(l: &mut Lower, c: SymbolId) -> bool {
    !has(l.u, c, TRAIT) && l.u.no_inits(c)
}

/// The method function part `fun` refers to.
fn callee(l: &Lower, fun: TNodeId) -> Option<SymbolId> {
    match *l.tpd.tree(fun) {
        Tree::TypeApply { fun, .. } => callee(l, fun),
        _ => l.tpd.sym(fun),
    }
}

/// dotty's `isPureApply`: a constant operation of a primitive class or `String`, a stable
/// constructor, or the synthetic `apply` of a case class that initializes nothing.
fn is_pure_apply(l: &mut Lower, t: TNodeId, fun: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let Some(s) = callee(l, fun) else { return false };
    let constant = matches!(l.u.types().get(l.tpd.tpe(t)), Type::Constant(..) | Type::Str(_));
    if constant && let Some(owner) = l.u.symbol(s).owner {
        let owner = l.u.full_name(owner);
        if PRIMITIVE_VALUE_CLASSES.contains(&owner.as_str()) || owner == "java.lang.String" {
            return true;
        }
    }
    if l.u.symbol(s).name == "<init>" {
        return has(l.u, s, STABLE);
    }
    is_pure_case_apply(l, s, inits)
}

/// dotty's `isPureSyntheticCaseApply`.
fn is_pure_case_apply(l: &mut Lower, s: SymbolId, inits: &FxHashSet<SymbolId>) -> bool {
    if l.u.symbol(s).name != "apply" || !has(l.u, s, SYNTHETIC) {
        return false;
    }
    let Some(module_class) = l.u.symbol(s).owner.filter(|&o| l.u.is_module_class(o)) else { return false };
    if l.u.source_module(module_class).is_some_and(|m| inits.contains(&m)) {
        return false;
    }
    let class = l.u.companion_class(module_class);
    class.is_some_and(|c| has(l.u, c, CASE) && is_no_inits_real_class(l, c))
}

/// Whether `t` applies the synthetic `apply` of a case class companion selected on a synthetic
/// object or imported synthetically (dotty's `isCaseClassApply`).
fn is_case_class_apply(l: &mut Lower, t: TNodeId, fun: TNodeId) -> bool {
    let Some(s) = callee(l, fun) else { return false };
    if l.u.symbol(s).name != "apply" || !has(l.u, s, SYNTHETIC) {
        return false;
    }
    let Some(owner) = l.u.symbol(s).owner.filter(|&o| l.u.is_module_class(o)) else { return false };
    if !l.u.companion_class(owner).is_some_and(|c| has(l.u, c, CASE)) {
        return false;
    }
    let tpe = l.tpd.tpe(t);
    if !l.u.class_symbol(tpe).is_some_and(|c| is_no_inits_real_class(l, c)) {
        return false;
    }
    let mut part = fun;
    while let Tree::TypeApply { fun, .. } = *l.tpd.tree(part) {
        part = fun;
    }
    match *l.tpd.tree(part) {
        Tree::Select { qualifier, .. } => l.tpd.sym(qualifier).is_some_and(|q| has(l.u, q, SYNTHETIC)),
        Tree::Ident { .. } => has(l.u, s, SYNTHETIC),
        _ => false,
    }
}
