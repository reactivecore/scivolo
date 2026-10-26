// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/localopt/DropForMap.scala
//! dotty's `DropForMap`: `r.map(x => x)` or `r.map(x => ())`, the trailing `map` of a `for`,
//! becomes `r` (or the value `r` was converted from) when that has the type of the call.

use super::casts::class_named;
use super::walk::{children, subtrees};
use crate::symbols::has;
use crate::{Lower, Result};
use tasty::tags::{EXTENSION, IMPLICIT};
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{List, TNodeId, Tree};

/// Drops the trailing `map` calls of `for` comprehensions in unit `root`, inner ones first; the
/// calls of `Inlined` trees are left as they are.
pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, root).into_iter().rev() {
        match *l.tpd.tree(t) {
            Tree::Apply { .. } => {
                if let Some(r) = dropped(l, t) {
                    l.copy_node(t, r);
                }
            }
            Tree::Inlined { call: Some(call), bindings, expansion } if marked_call(l, call) => {
                if let Some(e) = inlined_receiver(l, call, bindings, expansion) {
                    l.tpd.get_mut(t).tree = Tree::Inlined { call: Some(call), bindings, expansion: e };
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// What application `t` becomes without its trailing `map`, if it is one that may go.
fn dropped(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let tpe = l.tpd.tpe(t);
    if is_method_or_poly(l, tpe) {
        return None;
    }
    let (f0, sym, args) = unmapped(l, t)?;
    let f = if has(l.u, sym, EXTENSION) { *args.first()? } else { f0? };
    let widened = l.u.widen(l.tpd.tpe(f));
    if same_type(l, widened, tpe) {
        return Some(f);
    }
    converted(l, f).filter(|&r| {
        let rt = l.tpd.tpe(r);
        same_type(l, rt, tpe)
    })
}

/// The receiver, `map` method and arguments of `r.map(lambda)(implicits)` marked as trailing,
/// the mark removed.
fn unmapped(l: &mut Lower, t: TNodeId) -> Option<(Option<TNodeId>, SymbolId, Vec<TNodeId>)> {
    match *l.tpd.tree(t) {
        Tree::Apply { fun, args } if l.tpd.items(args).len() == 1 && is_lambda(l, l.tpd.items(args)[0]) => {
            if !l.tpd.is_trailing_map(t) {
                return None;
            }
            l.tpd.unmark_trailing_map(t);
            map_call(l, fun, Vec::new())
        }
        Tree::Apply { fun, .. } => {
            let implicit = matches!(l.u.types().get(l.tpd.tpe(fun)), Type::Method { implicit: true, .. });
            if implicit { unmapped(l, fun) } else { None }
        }
        Tree::TypeApply { fun, .. } => unmapped(l, fun),
        _ => None,
    }
}

/// The reference to the receiver's binding in `expansion` of inlined trailing `map` call `call`
/// with `bindings`, if the receiver has the expansion's type (dotty's
/// `DropForMap.transformInlined`).
fn inlined_receiver(l: &mut Lower, call: TNodeId, bindings: List, expansion: TNodeId) -> Option<TNodeId> {
    let (f0, sym, args) = call_parts(l, call)?;
    let f = if has(l.u, sym, EXTENSION) { *args.first()? } else { f0? };
    let widened = l.u.widen(l.tpd.tpe(f));
    let tpe = l.tpd.tpe(expansion);
    if !same_type(l, widened, tpe) {
        return converted(l, f).filter(|&r| {
            let rt = l.tpd.tpe(r);
            same_type(l, rt, tpe)
        });
    }
    let binding = l.tpd.items(bindings).iter().copied().find(|&b| match *l.tpd.tree(b) {
        Tree::ValDef { rhs: Some(rhs), .. } => same_tree(l, f, rhs),
        _ => false,
    })?;
    let proxy = l.tpd.sym(binding)?;
    subtrees(l.tpd, expansion).into_iter().find(|&t| match *l.tpd.tree(t) {
        Tree::Inlined { call: None, bindings, expansion: e } => {
            l.tpd.items(bindings).is_empty()
                && matches!(l.tpd.tree(e), Tree::Ident { .. })
                && l.tpd.sym(e) == Some(proxy)
        }
        _ => false,
    })
}

/// The receiver, `map` method and arguments of call `t` of `map` on a lambda.
fn call_parts(l: &Lower, t: TNodeId) -> Option<(Option<TNodeId>, SymbolId, Vec<TNodeId>)> {
    match *l.tpd.tree(t) {
        Tree::Apply { fun, args } if l.tpd.items(args).len() == 1 && is_lambda(l, l.tpd.items(args)[0]) => {
            map_call(l, fun, Vec::new())
        }
        Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => call_parts(l, fun),
        _ => None,
    }
}

/// Whether trees `a` and `b` are alike: of the same form, symbol and type, their children alike
/// (dotty's `sameTree`).
fn same_tree(l: &Lower, a: TNodeId, b: TNodeId) -> bool {
    let (x, y) = (l.tpd.get(a), l.tpd.get(b));
    let ca: Vec<TNodeId> = children(l.tpd, a).into_iter().collect();
    let cb: Vec<TNodeId> = children(l.tpd, b).into_iter().collect();
    std::mem::discriminant(&x.tree) == std::mem::discriminant(&y.tree)
        && x.sym == y.sym
        && x.tpe == y.tpe
        && ca.len() == cb.len()
        && ca.iter().zip(&cb).all(|(&c, &d)| same_tree(l, c, d))
}

/// The receiver, `map` method and innermost arguments of `t`, a call of `map`.
fn map_call(l: &Lower, t: TNodeId, args: Vec<TNodeId>) -> Option<(Option<TNodeId>, SymbolId, Vec<TNodeId>)> {
    let named_map = |l: &Lower, s: SymbolId| l.u.symbol(s).name == "map";
    match *l.tpd.tree(t) {
        Tree::Ident { .. } => {
            let sym = l.tpd.sym(t).filter(|&s| named_map(l, s) && has(l.u, s, EXTENSION))?;
            Some((None, sym, args))
        }
        Tree::Select { qualifier, .. } => {
            let sym = l.tpd.sym(t).filter(|&s| named_map(l, s))?;
            Some((Some(qualifier), sym, args))
        }
        Tree::Apply { fun, args: inner } => map_call(l, fun, l.tpd.items(inner).to_vec()),
        Tree::TypeApply { fun, .. } => map_call(l, fun, args),
        _ => None,
    }
}

/// Whether `t` is a closure over a method defined beside it without a type parameter clause.
fn is_lambda(l: &Lower, t: TNodeId) -> bool {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return false };
    let (&[def], Tree::Closure { meth, .. }) = (l.tpd.items(stats), l.tpd.tree(expr)) else { return false };
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(def) else { return false };
    let type_clause = l
        .tpd
        .clause_items(paramss)
        .iter()
        .any(|&c| l.tpd.items(c).iter().all(|&p| matches!(l.tpd.tree(p), Tree::TypeDef { .. })));
    l.tpd.sym(def).is_some() && l.tpd.sym(def) == l.tpd.sym(*meth) && !type_clause
}

/// The argument of `t` if it is an implicit conversion or `Conversion.apply` call.
fn converted(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    match *l.tpd.tree(t) {
        Tree::Apply { fun, .. } if matches!(l.tpd.tree(fun), Tree::Apply { .. }) => converted(l, fun),
        Tree::Apply { fun, args } => {
            let &[r] = l.tpd.items(args) else { return None };
            let fun = match *l.tpd.tree(fun) {
                Tree::TypeApply { fun, .. } => fun,
                _ => fun,
            };
            let s = l.tpd.sym(fun)?;
            let conversion = l.u.symbol(s).name == "apply"
                && match (l.u.symbol(s).owner, class_named(l, "scala.Conversion")) {
                    (Some(o), Some(c)) => l.u.derives_from(o, c),
                    _ => false,
                };
            (has(l.u, s, IMPLICIT) || conversion).then_some(r)
        }
        Tree::TypeApply { fun, .. } => converted(l, fun),
        _ => None,
    }
}

/// Whether `call`, past its applications, is a trailing `map` of a `for`.
fn marked_call(l: &Lower, call: TNodeId) -> bool {
    l.tpd.is_trailing_map(call)
        || match *l.tpd.tree(call) {
            Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => marked_call(l, fun),
            _ => false,
        }
}

fn is_method_or_poly(l: &Lower, tpe: TypeId) -> bool {
    matches!(l.u.types().get(tpe), Type::Method { .. } | Type::Poly { .. })
}

fn same_type(l: &mut Lower, a: TypeId, b: TypeId) -> bool {
    l.u.is_subtype(a, b) && l.u.is_subtype(b, a)
}
