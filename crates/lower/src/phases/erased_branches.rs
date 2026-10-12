//! The branches of conditionals, `try`s and switches typed by `Erasure`.

use super::adapt::erased_type;
use super::casts::type_of;
use super::erasure::typed;
use crate::{Lower, Result, unsupported};
use tasty::Erased;
use typer::tpd::{List, MatchKind, TNodeId, Tree};

/// `try expr catch cases finally finalizer` of `t`, its branches typed as `typed_branches`.
pub fn typed_try(
    l: &mut Lower,
    t: TNodeId,
    (expr, cases, finalizer): (TNodeId, List, Option<TNodeId>),
    pt: Option<&Erased>,
) -> Result<Tree> {
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
            l.recopy_class_literals(c);
        }
    }
    let finalizer = match finalizer {
        Some(f) => Some(typed(l, f, None)?),
        None => None,
    };
    Ok(Tree::Try { expr: typed_bodies[0], cases: l.tpd.list(&cases), finalizer })
}

/// Switch `selector match cases`, its selector and case bodies typed without an expected type.
pub fn typed_switch(l: &mut Lower, selector: TNodeId, cases: List) -> Result<Tree> {
    for c in l.tpd.items(cases).to_vec() {
        if let Tree::CaseDef { pat, guard, body } = *l.tpd.tree(c) {
            l.tpd.get_mut(c).tree = Tree::CaseDef { pat, guard, body: typed(l, body, None)? };
            l.recopy_class_literals(c);
        }
    }
    Ok(Tree::Match { selector: Some(typed(l, selector, None)?), cases, kind: MatchKind::Plain })
}

/// The branches of conditional or `try` `t`, typed against `pt` if a reference is expected,
/// `t` then of the erased lub of their classes; else against the erasure of `t`'s type.
pub fn typed_branches(
    l: &mut Lower,
    t: TNodeId,
    branches: &[TNodeId],
    pt: Option<&Erased>,
) -> Result<Vec<TNodeId>> {
    let reference = pt.filter(|p| matches!(p, Erased::Class(_) | Erased::Array(_)));
    let expected = match reference {
        Some(p) => p.clone(),
        None => l.u.erase_value(l.tpd.tpe(t)),
    };
    let mut out = Vec::new();
    for &b in branches {
        out.push(typed(l, b, Some(&expected))?);
    }
    if reference.is_some() {
        let mut classes: Vec<Erased> = Vec::new();
        for &b in &out {
            let e = erased_type(l, b);
            if !matches!(e, Erased::Nothing | Erased::Null) && !classes.contains(&e) {
                classes.push(e);
            }
        }
        let mut lub: Option<tasty::TypeId> = None;
        for class in &classes {
            if !matches!(class, Erased::Class(_)) {
                return unsupported(l.tpd.span(t), "branches of arrays and other erased classes");
            }
            let tpe = type_of(l, class);
            lub = Some(match lub {
                Some(acc) => {
                    let union = l.u.intern_type(tasty::Type::Or(acc, tpe, false));
                    let class = l.u.erase(union);
                    type_of(l, &class)
                }
                None => tpe,
            });
        }
        if let Some(lub) = lub {
            l.tpd.get_mut(t).tpe = Some(lub);
        }
    }
    Ok(out)
}
