//! The boxing of dotty's `Erasure`: each expression typed against the erasure of what is
//! expected there and adapted to it (`adapt.rs`).

use super::adapt::{adapt, applied_to_none, array_op, is_parameterless_ref};
use super::casts::{is_cast, receiver_type};
use super::erased_branches::{typed_branches, typed_switch, typed_try};
use crate::symbols::is_constructor;
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
                let pt = if is_constructor(l.u, sym) { Erased::Unit } else { l.u.erased_method(sym).1 };
                let rhs = typed(l, rhs, Some(&pt))?;
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
                stats.push(stat);
            }
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => stats.push(stat),
            _ => stats.push(typed(l, stat, None)?),
        }
    }
    let body = l.tpd.list(&stats[1..]);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `t` with its subtrees typed against their erased expected types, then adapted to `pt`.
pub fn typed(l: &mut Lower, t: TNodeId, pt: Option<&Erased>) -> Result<TNodeId> {
    let boolean = Erased::Prim('z');
    let tree = l.tpd.tree(t).clone();
    let new = match tree {
        Tree::Apply { mut fun, args } => {
            let mut all = l.tpd.items(args).to_vec();
            loop {
                match *l.tpd.tree(fun) {
                    Tree::Apply { fun: inner, args } => {
                        all.splice(0..0, l.tpd.items(args).iter().copied());
                        fun = inner;
                    }
                    Tree::TypeApply { fun: inner, .. } if !is_cast(l, fun) => fun = inner,
                    _ => break,
                }
            }
            if let Tree::Select { qualifier, name } = *l.tpd.tree(fun) {
                let owner = receiver_type(l, fun, qualifier);
                let qualifier = typed(l, qualifier, owner.as_ref())?;
                l.tpd.get_mut(fun).tree = Tree::Select { qualifier, name };
            }
            let params = match array_op(l, fun) {
                Some((params, _)) => params,
                None => l.tpd.sym(fun).map(|m| l.u.erased_method(m).0).unwrap_or_default(),
            };
            let mut typed_args = Vec::new();
            for (i, &a) in all.iter().enumerate() {
                typed_args.push(typed(l, a, params.get(i))?);
            }
            Some(Tree::Apply { fun, args: l.tpd.list(&typed_args) })
        }
        Tree::Select { qualifier, name } => {
            let owner = receiver_type(l, t, qualifier);
            Some(Tree::Select { qualifier: typed(l, qualifier, owner.as_ref())?, name })
        }
        Tree::TypeApply { fun, .. } if !is_cast(l, t) => return typed(l, fun, pt),
        Tree::TypeApply { fun, args } => {
            if let Tree::Select { qualifier, name } = *l.tpd.tree(fun) {
                let owner = receiver_type(l, fun, qualifier);
                let qualifier = typed(l, qualifier, owner.as_ref())?;
                l.tpd.get_mut(fun).tree = Tree::Select { qualifier, name };
            }
            Some(Tree::TypeApply { fun, args })
        }
        Tree::Block { stats, expr } => {
            let mut typed_stats = Vec::new();
            for s in l.tpd.items(stats).to_vec() {
                typed_stats.push(typed(l, s, None)?);
            }
            Some(Tree::Block { stats: l.tpd.list(&typed_stats), expr: typed(l, expr, pt)? })
        }
        Tree::If { cond, thenp, elsep } => {
            let cond = typed(l, cond, Some(&boolean))?;
            let branches = typed_branches(l, t, &[thenp, elsep], pt)?;
            Some(Tree::If { cond, thenp: branches[0], elsep: branches[1] })
        }
        Tree::WhileDo { cond, body } => {
            Some(Tree::WhileDo { cond: typed(l, cond, Some(&boolean))?, body: typed(l, body, None)? })
        }
        Tree::Return { expr, from } => {
            let result = l.tpd.sym(from).map(|m| match l.u.symbol(m).kind {
                Kind::Def => l.u.erased_method(m).1,
                _ => {
                    let info = l.u.info(m);
                    l.u.erase(info)
                }
            });
            Some(Tree::Return { expr: typed(l, expr, result.as_ref())?, from })
        }
        Tree::Labeled { expr } => {
            let erased = l.u.erase(l.tpd.tpe(t));
            Some(Tree::Labeled { expr: typed(l, expr, Some(&erased))? })
        }
        Tree::Match { selector, cases } => Some(typed_switch(l, selector, cases)?),
        Tree::Assign { lhs, rhs } => {
            let erased = l.u.erase(l.tpd.tpe(lhs));
            Some(Tree::Assign { lhs, rhs: typed(l, rhs, Some(&erased))? })
        }
        Tree::SeqLiteral { elems, elemtpt } => {
            let elem = match l.u.erase(l.tpd.tpe(t)) {
                Erased::Array(e) => *e,
                _ => return unsupported(l.tpd.span(t), "a sequence literal that is not an array"),
            };
            let mut typed_elems = Vec::new();
            for e in l.tpd.items(elems).to_vec() {
                typed_elems.push(typed(l, e, Some(&elem))?);
            }
            Some(Tree::SeqLiteral { elems: l.tpd.list(&typed_elems), elemtpt })
        }
        Tree::Typed { expr, tpt } => {
            let ascribed = l.u.erase(l.tpd.tpe(tpt));
            Some(Tree::Typed { expr: typed(l, expr, Some(&ascribed))?, tpt })
        }
        Tree::ValDef { name, tpt, rhs: Some(rhs) } => {
            let sym = l.tpd.sym(t).expect("a value definition's symbol");
            let info = l.u.info(sym);
            let erased = l.u.erase(info);
            Some(Tree::ValDef { name, tpt, rhs: Some(typed(l, rhs, Some(&erased))?) })
        }
        Tree::Try { expr, cases, finalizer } => Some(typed_try(l, t, (expr, cases, finalizer), pt)?),
        _ => None,
    };
    if let Some(new) = new {
        l.tpd.get_mut(t).tree = new;
    }
    let t = if is_parameterless_ref(l, t) { applied_to_none(l, t) } else { t };
    Ok(match pt {
        Some(pt) => adapt(l, t, pt),
        None => t,
    })
}
