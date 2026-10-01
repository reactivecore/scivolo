//! The boxing of dotty's `Erasure`: each expression typed against the erasure of what is
//! expected there and adapted to it (`adapt.rs`).

use super::adapt::{adapt, applied_to_none, is_parameterless_ref};
use crate::symbols::is_constructor;
use crate::{Lower, Result};
use tasty::Erased;
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
                let rhs = typed(l, rhs, Some(&pt));
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
                stats.push(stat);
            }
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => stats.push(stat),
            _ => stats.push(typed(l, stat, None)),
        }
    }
    let body = l.tpd.list(&stats[1..]);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `t` with its subtrees typed against their erased expected types, then adapted to `pt`.
fn typed(l: &mut Lower, t: TNodeId, pt: Option<&Erased>) -> TNodeId {
    let boolean = Erased::Prim('z');
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
        Tree::If { cond, thenp, elsep } => {
            let erased = l.u.erase(l.tpd.tpe(t));
            let cond = typed(l, cond, Some(&boolean));
            Some(Tree::If {
                cond,
                thenp: typed(l, thenp, Some(&erased)),
                elsep: typed(l, elsep, Some(&erased)),
            })
        }
        Tree::WhileDo { cond, body } => {
            Some(Tree::WhileDo { cond: typed(l, cond, Some(&boolean)), body: typed(l, body, None) })
        }
        Tree::Return { expr, from } => {
            let result = l.tpd.sym(from).map(|m| l.u.erased_method(m).1);
            Some(Tree::Return { expr: typed(l, expr, result.as_ref()), from })
        }
        Tree::Assign { lhs, rhs } => {
            let erased = l.u.erase(l.tpd.tpe(lhs));
            Some(Tree::Assign { lhs, rhs: typed(l, rhs, Some(&erased)) })
        }
        Tree::Typed { expr, tpt } => {
            let ascribed = l.u.erase(l.tpd.tpe(tpt));
            Some(Tree::Typed { expr: typed(l, expr, Some(&ascribed)), tpt })
        }
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
