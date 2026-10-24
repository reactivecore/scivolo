// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MoveStatics.scala
//! dotty's `MoveStatics` for methods: each `@static` method of an object moves to the companion
//! class as a static method; `@static` values, and methods whose bodies need the object's `this`
//! or define local methods or classes, are refused.

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
        for &stat in &statics {
            check_movable(l, stat)?;
        }
        let class = companion.expect("a companion class");
        let kept: Vec<TNodeId> = body(l, td).into_iter().filter(|s| !statics.contains(s)).collect();
        set_body(l, td, &kept);
        let mut target_body = body(l, target);
        target_body.extend_from_slice(&statics);
        set_body(l, target, &target_body);
        for &stat in &statics {
            let m = l.tpd.sym(stat).expect("a method");
            l.u.set_owner(m, class);
            l.u.enter(m);
            let flags = l.u.symbol(m).flags.with(STATIC);
            l.u.set_flags(m, flags);
            moved.insert(m);
        }
    }
    for t in subtrees(l.tpd, root) {
        let Some(s) = l.tpd.sym(t) else { continue };
        if moved.contains(&s) && matches!(l.tpd.tree(t), Tree::Select { .. }) {
            let span = l.tpd.span(t);
            l.tpd.get_mut(t).span = span.with_point(span.start());
        } else if !moved.contains(&s) && is_static(l, s) {
            return unsupported(l.tpd.span(t), "@static members");
        }
    }
    Ok(())
}

fn is_static(l: &Lower, s: SymbolId) -> bool {
    l.u.is_annotated(s, STATIC_ANNOT)
}

/// Refuses `@static` definition `stat` unless it is a method whose body needs no `this` and
/// defines no method but closures' of function types and no class.
fn check_movable(l: &Lower, stat: TNodeId) -> Result<()> {
    let Tree::DefDef { rhs, .. } = *l.tpd.tree(stat) else {
        return unsupported(l.tpd.span(stat), "@static values");
    };
    for t in rhs.map(|r| subtrees(l.tpd, r)).unwrap_or_default() {
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
    let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { return Vec::new() };
    match *l.tpd.tree(rhs) {
        Tree::Template { body, .. } => l.tpd.items(body).to_vec(),
        _ => Vec::new(),
    }
}

fn set_body(l: &mut Lower, td: TNodeId, stats: &[TNodeId]) {
    let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { return };
    let list = l.tpd.list(stats);
    if let Tree::Template { body, .. } = &mut l.tpd.get_mut(rhs).tree {
        *body = list;
    }
}
