//! The boxing of dotty's `Erasure`: each expression typed against the erasure of what is
//! expected there, a primitive value where a reference is expected boxed by its object's `box`,
//! `()` there replaced by `BoxedUnit.UNIT`.

use crate::symbols::{has, is_constructor};
use crate::{Lower, Result};
use tasty::tags::FIELDaccessor;
use tasty::{Erased, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Types the methods of class definition `td` against their erased result types.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, body, .. } = *l.tpd.tree(rhs) else { return Ok(()) };
    let stats: Vec<TNodeId> = std::iter::once(constr).chain(l.tpd.items(body).iter().copied()).collect();
    for stat in stats {
        let Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) } = *l.tpd.tree(stat) else { continue };
        let sym = l.tpd.sym(stat).expect("a method definition's symbol");
        let pt = if is_constructor(l.u, sym) { Erased::Unit } else { l.u.erased_method(sym).1 };
        let rhs = typed(l, rhs, Some(&pt));
        l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    }
    Ok(())
}

/// `t` with its subtrees typed against their erased expected types, then adapted to `pt`.
fn typed(l: &mut Lower, t: TNodeId, pt: Option<&Erased>) -> TNodeId {
    let tree = l.tpd.tree(t).clone();
    let new = match tree {
        Tree::Apply { fun, args } => {
            if let Tree::Select { qualifier, name } = *l.tpd.tree(fun) {
                let qualifier = typed(l, qualifier, None);
                l.tpd.get_mut(fun).tree = Tree::Select { qualifier, name };
            }
            let params = l.tpd.sym(fun).map(|m| l.u.erased_method(m).0).unwrap_or_default();
            let args: Vec<TNodeId> = l.tpd.items(args).to_vec();
            let args: Vec<TNodeId> =
                args.iter().enumerate().map(|(i, &a)| typed(l, a, params.get(i))).collect();
            Some(Tree::Apply { fun, args: l.tpd.list(&args) })
        }
        Tree::Select { qualifier, name } => Some(Tree::Select { qualifier: typed(l, qualifier, None), name }),
        Tree::Block { stats, expr } => {
            let stats: Vec<TNodeId> = l.tpd.items(stats).to_vec();
            let stats: Vec<TNodeId> = stats.into_iter().map(|s| typed(l, s, None)).collect();
            Some(Tree::Block { stats: l.tpd.list(&stats), expr: typed(l, expr, pt) })
        }
        Tree::Typed { expr, tpt } => Some(Tree::Typed { expr: typed(l, expr, pt), tpt }),
        Tree::ValDef { name, tpt, rhs: Some(rhs) } => {
            let sym = l.tpd.sym(t).expect("a value definition's symbol");
            let info = l.u.info(sym);
            let erased = l.u.erase(info);
            Some(Tree::ValDef { name, tpt, rhs: Some(typed(l, rhs, Some(&erased))) })
        }
        Tree::Try { expr, cases, finalizer } => {
            let erased = l.u.erase(l.tpd.tpe(t));
            let expr = typed(l, expr, Some(&erased));
            let cases: Vec<TNodeId> = l.tpd.items(cases).to_vec();
            for &c in &cases {
                if let Tree::CaseDef { pat, guard, body } = *l.tpd.tree(c) {
                    let body = typed(l, body, Some(&erased));
                    l.tpd.get_mut(c).tree = Tree::CaseDef { pat, guard, body };
                }
            }
            let finalizer = finalizer.map(|f| typed(l, f, None));
            Some(Tree::Try { expr, cases: l.tpd.list(&cases), finalizer })
        }
        _ => None,
    };
    if let Some(new) = new {
        l.tpd.get_mut(t).tree = new;
    }
    let t = if is_parameterless_ref(l, t) { applied_to_none(l, t) } else { t };
    match pt {
        Some(pt) => adapt(l, t, pt),
        None => t,
    }
}

/// Whether `t` refers to a method without parameter lists: a getter or a parameterless `def`.
fn is_parameterless_ref(l: &mut Lower, t: TNodeId) -> bool {
    let (Tree::Select { .. } | Tree::Ident { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else {
        return false;
    };
    let method = l.u.symbol(s).kind == Kind::Def || has(l.u, s, FIELDaccessor);
    let info = l.u.info(s);
    method
        && !is_constructor(l.u, s)
        && !matches!(l.u.types().get(info), Type::Method { .. } | Type::Poly { .. })
}

/// `t()`, spanning `t` with its start as point.
fn applied_to_none(l: &mut Lower, t: TNodeId) -> TNodeId {
    let span = l.tpd.span(t);
    let span = if span.exists() { span.with_point(span.start()) } else { span };
    let tpe = l.tpd.tpe(t);
    l.apply(t, &[], tpe, span)
}

/// `t` adapted to erased type `pt`: boxed if a primitive value or `()` where a reference is
/// expected.
fn adapt(l: &mut Lower, t: TNodeId, pt: &Erased) -> TNodeId {
    if !matches!(pt, Erased::Class(_) | Erased::Array(_)) {
        return t;
    }
    let tpe = l.tpd.tpe(t);
    match l.u.erase(tpe) {
        Erased::Prim(c) => match l.defs.primitive(c) {
            Some(p) => {
                let (module, box_method) = (p.module, p.box_method);
                boxed(l, t, module, box_method)
            }
            None => t,
        },
        Erased::Unit => boxed_unit(l, t),
        _ => t,
    }
}

/// `C.box(t)` for object `C` with method `box_method`.
fn boxed(l: &mut Lower, t: TNodeId, module: SymbolId, box_method: SymbolId) -> TNodeId {
    let span = l.tpd.span(t);
    let module_ref = l.ident(module, span);
    let fun = l.select(module_ref, box_method, span);
    let info = l.u.info(box_method);
    let result = l.u.result_type(info);
    l.apply(fun, &[t], result, span)
}

/// `BoxedUnit.UNIT` for unit value `t`, after `t` unless it is pure.
fn boxed_unit(l: &mut Lower, t: TNodeId) -> TNodeId {
    let span = l.tpd.span(t);
    let unit = l.defs.boxed_unit_unit;
    let unit = l.ident(unit, span);
    match l.tpd.tree(t) {
        Tree::Literal { .. } | Tree::Ident { .. } | Tree::This { .. } => unit,
        _ => l.block(&[t], unit, span),
    }
}
