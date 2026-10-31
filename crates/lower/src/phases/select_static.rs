// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SelectStatic.scala
//! dotty's `SelectStatic` for enum cases and objects nested in static objects: `qual.f` becomes
//! `f` if `qual` is a path of static objects, else `{ qual; f }`, without a span; the enclosing
//! tree is copied as dotty's tree copiers do, which gives it one.

use super::walk::{children, subtrees};
use crate::Lower;
use crate::symbols::{has, is_jdk_object, is_static};
use syntax::ast::Span;
use tasty::tags::{ENUM, STATIC};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) {
    for parent in subtrees(l.tpd, td) {
        if let Tree::Template { parents, .. } = *l.tpd.tree(parent) {
            for p in l.tpd.items(parents).to_vec() {
                super_ref_span(l, p);
            }
        }
        let mut changed = false;
        for c in children(l.tpd, parent) {
            let Tree::Select { qualifier, .. } = *l.tpd.tree(c) else { continue };
            let Some(f) = l.tpd.sym(c) else { continue };
            if !is_nested_static_module(l, f) && !is_static_enum_value(l, f)
                || is_java_module_ref(l, qualifier)
            {
                continue;
            }
            let tpe = l.tpd.tpe(c);
            let r = l.ident(f, Span::NONE);
            l.tpd.get_mut(r).tpe = Some(tpe);
            let new = if is_static_owner_ref(l, qualifier) {
                r
            } else {
                let block = l.block(&[qualifier], r, Span::NONE);
                l.positioned(block)
            };
            l.copy_node(c, new);
            changed = true;
        }
        if changed {
            l.recopy(parent);
        }
    }
}

/// Gives the constructor of superclass call `call` the call's span, as `Mixin`'s `superRef` does
/// before.
fn super_ref_span(l: &mut Lower, call: TNodeId) {
    let Tree::Apply { fun, .. } = *l.tpd.tree(call) else { return };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return };
    let span = l.tpd.span(call);
    if matches!(l.tpd.tree(qualifier), Tree::New { .. }) && span.exists() {
        l.tpd.get_mut(fun).span = span;
    }
}

/// Whether `f` is the static field of an enum case.
pub fn is_static_enum_value(l: &Lower, f: SymbolId) -> bool {
    l.u.symbol(f).kind == Kind::Val && has(l.u, f, STATIC) && has(l.u, f, ENUM)
}

/// Whether `t` refers to the object of a Java class (dotty's `JavaModule`).
fn is_java_module_ref(l: &mut Lower, t: TNodeId) -> bool {
    let module = l.tpd.sym(t).and_then(|s| l.u.module_class(s));
    module.is_some_and(|c| is_jdk_object(l.u, c))
}

/// Whether `s` is an object nested in a static object.
fn is_nested_static_module(l: &Lower, s: SymbolId) -> bool {
    let owner = l.u.symbol(s).owner;
    l.u.is_module(s) && owner.is_some_and(|o| l.u.symbol(o).kind != Kind::Package) && is_static(l.u, s)
}

/// Whether `t` is a path of packages and static objects (dotty's `isStaticOwnerRef`).
pub fn is_static_owner_ref(l: &mut Lower, t: TNodeId) -> bool {
    let prefix = match *l.tpd.tree(t) {
        Tree::Ident { .. } => true,
        Tree::Select { qualifier, .. } => is_static_owner_ref(l, qualifier),
        _ => false,
    };
    let Some(s) = l.tpd.sym(t).filter(|_| prefix) else { return false };
    if l.u.symbol(s).kind == Kind::Package {
        return true;
    }
    l.u.is_module(s) && l.u.module_class(s).is_some_and(|c| is_static(l.u, c))
}
