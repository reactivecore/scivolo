//! The boxing of dotty's `Erasure`: each expression typed against the erasure of what is
//! expected there and adapted to it (`adapt.rs`).

use super::adapt::{adapt, applied_to_none, erased_type, is_parameterless_ref};
use crate::symbols::is_constructor;
use crate::{Lower, Result, unsupported};
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
fn typed(l: &mut Lower, t: TNodeId, pt: Option<&Erased>) -> Result<TNodeId> {
    let boolean = Erased::Prim('z');
    let tree = l.tpd.tree(t).clone();
    let new = match tree {
        Tree::Apply { fun, args } => {
            if let Tree::Select { qualifier, name } = *l.tpd.tree(fun) {
                let qualifier = typed(l, qualifier, None)?;
                l.tpd.get_mut(fun).tree = Tree::Select { qualifier, name };
            }
            let params = l.tpd.sym(fun).map(|m| l.u.erased_method(m).0).unwrap_or_default();
            let mut typed_args = Vec::new();
            for (i, &a) in l.tpd.items(args).to_vec().iter().enumerate() {
                typed_args.push(typed(l, a, params.get(i))?);
            }
            Some(Tree::Apply { fun, args: l.tpd.list(&typed_args) })
        }
        Tree::Select { qualifier, name } => {
            Some(Tree::Select { qualifier: typed(l, qualifier, None)?, name })
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
            let result = l.tpd.sym(from).map(|m| l.u.erased_method(m).1);
            Some(Tree::Return { expr: typed(l, expr, result.as_ref())?, from })
        }
        Tree::Assign { lhs, rhs } => {
            let erased = l.u.erase(l.tpd.tpe(lhs));
            Some(Tree::Assign { lhs, rhs: typed(l, rhs, Some(&erased))? })
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
        Tree::Try { expr, cases, finalizer } => {
            let cases: Vec<TNodeId> = l.tpd.items(cases).to_vec();
            let mut bodies = vec![expr];
            for &c in &cases {
                if let Tree::CaseDef { body, .. } = *l.tpd.tree(c) {
                    bodies.push(body);
                }
            }
            let typed_bodies = typed_branches(l, t, &bodies, pt)?;
            for (&c, &body) in cases.iter().zip(&typed_bodies[1..]) {
                if let Tree::CaseDef { pat, guard, .. } = *l.tpd.tree(c) {
                    l.tpd.get_mut(c).tree = Tree::CaseDef { pat, guard, body };
                }
            }
            let finalizer = match finalizer {
                Some(f) => Some(typed(l, f, None)?),
                None => None,
            };
            Some(Tree::Try { expr: typed_bodies[0], cases: l.tpd.list(&cases), finalizer })
        }
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

/// The branches of conditional or `try` `t`, typed against `pt` if a reference is expected,
/// `t` then of their common erased class; else against the erasure of `t`'s type.
fn typed_branches(
    l: &mut Lower,
    t: TNodeId,
    branches: &[TNodeId],
    pt: Option<&Erased>,
) -> Result<Vec<TNodeId>> {
    let reference = pt.filter(|p| matches!(p, Erased::Class(_) | Erased::Array(_)));
    let expected = match reference {
        Some(p) => p.clone(),
        None => l.u.erase(l.tpd.tpe(t)),
    };
    let mut out = Vec::new();
    for &b in branches {
        out.push(typed(l, b, Some(&expected))?);
    }
    if reference.is_some() {
        let mut classes: Vec<Erased> = Vec::new();
        for &b in &out {
            let e = erased_type(l, b);
            if e != Erased::Nothing && !classes.contains(&e) {
                classes.push(e);
            }
        }
        match classes.as_slice() {
            [] => {}
            [Erased::Class(name)] => {
                let tpe = l.u.class_ref(name);
                l.tpd.get_mut(t).tpe = Some(tpe);
            }
            _ => return unsupported(l.tpd.span(t), "branches of different erased classes"),
        }
    }
    Ok(out)
}
