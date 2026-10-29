// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MoveStatics.scala
//! dotty's `MoveStatics`: each `@static` method of an object moves to the companion class as a
//! static method, its values become static fields (`static_values.rs`); methods whose bodies
//! need the object's `this` or define local methods or classes are refused.

use super::walk::{class_defs, subtrees};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::SymbolId;
use tasty::tags::STATIC;
use typer::tpd::{TNodeId, Tree};

const STATIC_ANNOT: &str = "scala.annotation.static";

pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    let classes = class_defs(l.tpd, root);
    let mut moved = FxHashSet::default();
    for &td in &classes {
        let Some(module) = l.tpd.sym(td).filter(|&c| l.u.is_module_class(c)) else { continue };
        let statics: Vec<TNodeId> =
            body(l, td).into_iter().filter(|&s| l.tpd.sym(s).is_some_and(|m| is_static(l, m))).collect();
        if statics.is_empty() {
            continue;
        }
        let companion = l.u.companion_class(module);
        let target = classes.iter().copied().find(|&c| l.tpd.sym(c).is_some() && l.tpd.sym(c) == companion);
        let plain = companion.is_some_and(|c| !l.u.is_trait(c) && l.u.underlying(c).is_none());
        let Some(target) = target.filter(|_| plain) else {
            return unsupported(
                l.tpd.span(statics[0]),
                "@static members of an object whose companion is a trait, a value class or in another unit",
            );
        };
        let class = companion.expect("a companion class");
        let module_body = body(l, td);
        let values = super::static_values::transform(l, (module, class, target), (&statics, &module_body))?;
        let mut statics: Vec<TNodeId> =
            statics.into_iter().filter(|&s| matches!(l.tpd.tree(s), Tree::DefDef { .. })).collect();
        for &stat in &statics {
            check_movable(l, stat)?;
        }
        for v in values {
            if !statics.contains(&v) {
                statics.push(v);
            }
        }
        let kept: Vec<TNodeId> = module_body.into_iter().filter(|s| !statics.contains(s)).collect();
        set_body(l, td, &kept);
        let mut target_body = body(l, target);
        target_body.extend_from_slice(&statics);
        set_body(l, target, &target_body);
        for &stat in &statics {
            let m = l.tpd.sym(stat).expect("a method");
            if l.u.symbol(m).owner != Some(class) {
                l.u.set_owner(m, class);
                l.u.enter(m);
            }
            let flags = l.u.symbol(m).flags.with(STATIC);
            l.u.set_flags(m, flags);
            moved.insert(m);
        }
    }
    for t in subtrees(l.tpd, root) {
        let Some(s) = l.tpd.sym(t) else { continue };
        let static_value = is_static(l, s) && l.u.symbol(s).flags.has(STATIC);
        if (moved.contains(&s) || static_value) && matches!(l.tpd.tree(t), Tree::Select { .. }) {
            let span = l.tpd.span(t);
            l.tpd.get_mut(t).span = span.with_point(span.start());
        } else if !moved.contains(&s)
            && l.u.symbol(s).tree.is_none()
            && is_static(l, s)
            && !l.u.symbol(s).flags.has(STATIC)
        {
            return unsupported(l.tpd.span(t), "@static members");
        }
    }
    Ok(())
}

fn is_static(l: &mut Lower, s: SymbolId) -> bool {
    l.u.is_annotated(s, STATIC_ANNOT)
}

/// Refuses `@static` method definition `stat` unless its body needs no `this` and defines no
/// method but closures' of function types and no class.
fn check_movable(l: &Lower, stat: TNodeId) -> Result<()> {
    let Tree::DefDef { rhs, .. } = *l.tpd.tree(stat) else { unreachable!("a method") };
    rhs.map_or(Ok(()), |r| check_needs_no_this(l, r))
}

/// Refuses `t` if it needs the object's `this` or defines a method but closures' of function
/// types or a class.
pub fn check_needs_no_this(l: &Lower, t: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, t) {
        let closure = l.tpd.sym(t).is_some_and(|s| l.u.symbol(s).name == "$anonfun");
        let local_def = matches!(l.tpd.tree(t), Tree::DefDef { .. }) && !closure;
        let sam = matches!(l.tpd.tree(t), Tree::Closure { tpt: Some(_), .. });
        let refused = matches!(l.tpd.tree(t), Tree::This { .. } | Tree::TypeDef { .. } | Tree::Super { .. });
        if local_def || sam || refused {
            return unsupported(
                l.tpd.span(t),
                "@static method needing its object or defining local methods or classes",
            );
        }
    }
    Ok(())
}

fn body(l: &Lower, td: TNodeId) -> Vec<TNodeId> {
    l.tpd.class_template(td).map_or_else(Vec::new, |t| l.tpd.items(t.body).to_vec())
}

fn set_body(l: &mut Lower, td: TNodeId, stats: &[TNodeId]) {
    let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { return };
    let list = l.tpd.list(stats);
    if let Tree::Template { body, .. } = &mut l.tpd.get_mut(rhs).tree {
        *body = list;
    }
}
