//! The boxing of dotty's `Erasure`: each expression typed against the erasure of what is
//! expected there and adapted to it (`adapt.rs`).

mod calls;

use super::adapt::{adapt, applied_to_none, erased_type, is_parameterless_ref};
use super::casts::receiver_type;
use super::erased_branches::{typed_branches, typed_switch, typed_try};
use super::type_tests::{intercept, is_test_or_cast};
use super::walk::map_children;
use crate::{Lower, Result, unsupported};
use tasty::{Erased, Kind};
use typer::tpd::{TNodeId, Tree};

/// Types the methods of class definition `td` against their erased result types, and the
/// statements and field initializers of its body.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut stats = Vec::new();
    for stat in std::iter::once(constr).chain(l.tpd.items(body).iter().copied()).collect::<Vec<_>>() {
        match *l.tpd.tree(stat) {
            Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) } => {
                let sym = l.tpd.sym(stat).expect("a method definition's symbol");
                let pt = if l.u.is_constructor(sym) { Erased::Unit } else { l.u.erased_method(sym).1 };
                let before = l.tpd.span(rhs);
                let rhs = typed(l, rhs, Some(&pt))?;
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
                if l.u.symbol(sym).kind == Kind::Val
                    || crate::symbols::has(l.u, sym, tasty::tags::FIELDaccessor)
                {
                    keep_class_literal_start(l, stat, before);
                }
                l.recopy_class_literals(stat);
                stats.push(stat);
            }
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => stats.push(stat),
            _ => stats.push(typed(l, stat, None)?),
        }
    }
    let body = l.tpd.list(&stats[1..]);
    let mut typed_parents = Vec::new();
    for parent in l.tpd.items(parents).to_vec() {
        let call = matches!(l.tpd.tree(parent), Tree::Apply { .. } | Tree::Block { .. });
        typed_parents.push(if call { typed(l, parent, None)? } else { parent });
    }
    let parents = l.tpd.list(&typed_parents);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// Qualifier `qualifier` of selection `sel` typed without an expected type, then adapted to the
/// class the selected member is called on.
fn typed_qualifier(l: &mut Lower, sel: TNodeId, qualifier: TNodeId) -> Result<TNodeId> {
    let owner = receiver_type(l, sel, qualifier);
    let qualifier = typed(l, qualifier, None)?;
    Ok(match owner {
        Some(owner) => adapt(l, qualifier, &owner),
        None => qualifier,
    })
}

/// `t` with its subtrees typed against their erased expected types, then adapted to `pt`.
pub fn typed(l: &mut Lower, t: TNodeId, pt: Option<&Erased>) -> Result<TNodeId> {
    let boolean = Erased::Prim('z');
    if is_test_or_cast(l, t) {
        intercept(l, t)?;
    }
    let new = match *l.tpd.tree(t) {
        Tree::Apply { fun, args } => Some(calls::typed_apply(l, t, fun, args)?),
        Tree::Select { qualifier, name } => {
            Some(Tree::Select { qualifier: typed_qualifier(l, t, qualifier)?, name })
        }
        Tree::TypeApply { fun, .. } if !is_test_or_cast(l, t) => return typed(l, fun, pt),
        Tree::TypeApply { fun, args } => {
            if let Tree::Select { qualifier, name } = *l.tpd.tree(fun) {
                let qualifier = typed_qualifier(l, fun, qualifier)?;
                l.tpd.get_mut(fun).tree = Tree::Select { qualifier, name };
            }
            Some(Tree::TypeApply { fun, args })
        }
        Tree::Block { stats, expr } => {
            let kept: Vec<_> = l
                .tpd
                .items(stats)
                .iter()
                .copied()
                .filter(|&s| !matches!(l.tpd.tree(s), Tree::Import { .. }))
                .collect();
            l.tpd.get_mut(t).tree = Tree::Block { stats: l.tpd.list(&kept), expr };
            map_children(l, t, &mut |l, c| typed(l, c, if c == expr { pt } else { None }))?;
            None
        }
        Tree::If { cond, thenp, elsep } => {
            let cond = typed(l, cond, Some(&boolean))?;
            let branches = typed_branches(l, t, &[thenp, elsep], pt)?;
            Some(Tree::If { cond, thenp: branches[0], elsep: branches[1] })
        }
        Tree::WhileDo { cond, .. } => {
            map_children(l, t, &mut |l, c| typed(l, c, if c == cond { Some(&boolean) } else { None }))?;
            None
        }
        Tree::Return { expr, from } => {
            if is_unit_member(l, expr) {
                return unsupported(l.tpd.span(t), "returns of Unit values of class members");
            }
            let result = l.tpd.sym(from).map(|m| match l.u.symbol(m).kind {
                Kind::Def => l.u.erased_method(m).1,
                _ => {
                    let info = l.u.info(m);
                    l.u.erase(info)
                }
            });
            typed_except(l, t, from, result.as_ref())?;
            None
        }
        Tree::Labeled { .. } => {
            let erased = l.u.erase(l.tpd.tpe(t));
            map_children(l, t, &mut |l, c| typed(l, c, Some(&erased)))?;
            None
        }
        Tree::Match { selector, cases } => Some(typed_switch(l, selector, cases)?),
        Tree::Assign { lhs, .. } => {
            let erased = erased_type(l, lhs);
            typed_except(l, t, lhs, Some(&erased))?;
            None
        }
        Tree::SeqLiteral { elemtpt, .. } => {
            let elem = match l.u.erase(l.tpd.tpe(t)) {
                Erased::Array(e) => *e,
                _ => return unsupported(l.tpd.span(t), "a sequence literal that is not an array"),
            };
            typed_except(l, t, elemtpt, Some(&elem))?;
            None
        }
        Tree::Typed { expr, tpt } => {
            let ascribed = l.u.erase(l.tpd.tpe(tpt));
            let written = !matches!(l.tpd.tree(tpt), Tree::TypeTree { inferred: true });
            if written && ascribed == Erased::Unit && !matches!(l.tpd.tree(expr), Tree::Literal { .. }) {
                return unsupported(l.tpd.span(t), "ascriptions to Unit of values other than literals");
            }
            typed_except(l, t, tpt, Some(&ascribed))?;
            None
        }
        Tree::ValDef { tpt, rhs: Some(rhs), .. } => {
            let sym = l.tpd.sym(t).expect("a value definition's symbol");
            let info = l.u.info(sym);
            let erased = match l.u.erase(info) {
                Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
                e => e,
            };
            let span = l.tpd.span(rhs);
            typed_except(l, t, tpt, Some(&erased))?;
            keep_class_literal_start(l, t, span);
            None
        }
        Tree::Try { expr, cases, finalizer } => Some(typed_try(l, t, (expr, cases, finalizer), pt)?),
        Tree::ClassLiteral { .. } => {
            l.tpd.get_mut(t).span = syntax::ast::Span::NONE;
            None
        }
        _ => None,
    };
    if let Some(new) = new {
        l.tpd.get_mut(t).tree = new;
    }
    l.recopy_class_literals(t);
    let t = if is_parameterless_ref(l, t) { applied_to_none(l, t) } else { t };
    Ok(match pt {
        Some(pt) => adapt(l, t, pt),
        None => t,
    })
}

/// Gives the right-hand side of value definition `t` the start of its span `before` erasure if
/// it is a class literal.
fn keep_class_literal_start(l: &mut Lower, t: TNodeId, before: syntax::ast::Span) {
    let (Tree::ValDef { rhs: Some(rhs), .. } | Tree::DefDef { rhs: Some(rhs), .. }) = *l.tpd.tree(t) else {
        return;
    };
    if matches!(l.tpd.tree(rhs), Tree::ClassLiteral { .. }) && !l.tpd.span(rhs).exists() && before.exists() {
        l.tpd.get_mut(rhs).span = before.start_pos();
    }
}

/// Whether `t` refers to a `Unit` value or getter of a class.
fn is_unit_member(l: &mut Lower, t: TNodeId) -> bool {
    let (Tree::Ident { .. } | Tree::Select { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else {
        return false;
    };
    let in_class = l.u.symbol(s).owner.is_some_and(|o| l.u.is_class(o));
    let info = l.u.info(s);
    let result = l.u.result_type(info);
    in_class && l.u.symbol(s).kind.is_term() && l.u.erase(result) == Erased::Unit
}

/// Types each child of `t` but `kept` against `pt`, rebuilding `t` in place.
fn typed_except(l: &mut Lower, t: TNodeId, kept: TNodeId, pt: Option<&Erased>) -> Result<()> {
    map_children(l, t, &mut |l, c| if c == kept { Ok(c) } else { typed(l, c, pt) })
}
