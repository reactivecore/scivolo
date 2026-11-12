// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SelectStatic.scala
//   compiler/src/dotty/tools/dotc/transform/Mixin.scala
//! dotty's `SelectStatic` for enum cases and objects nested in static objects but of a Java
//! class: `qual.f` becomes `f` if `qual` is a path of static objects, else `{ qual; f }`, without
//! a span; the enclosing tree is copied as dotty's tree copiers do, which gives it one. The same
//! for `@static` members but those of a path, which stay selections. Also the span `Mixin` gives a
//! superclass constructor call.

use super::walk::{children, subtrees};
use crate::Lower;
use crate::symbols::{has, is_jdk_object, is_static};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::tags::{ENUM, FIELDaccessor, LAZY, STATIC};
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
            if !is_nested_static_module(l, f) && !is_static_enum_value(l, f) && !is_scala_static(l, f)
                || is_java_module_ref(l, qualifier)
            {
                continue;
            }
            let tpe = l.tpd.tpe(c);
            let owner_ref = is_static_owner_ref(l, qualifier);
            if owner_ref && is_scala_static(l, f) {
                continue;
            }
            let at = if owner_ref { moved_init_span(l, parent, c) } else { None };
            let r = l.ident(f, at.unwrap_or(Span::NONE));
            l.tpd.get_mut(r).tpe = Some(tpe);
            let new = if owner_ref {
                r
            } else {
                let block = l.block(&[qualifier], r, Span::NONE);
                l.positioned(block)
            };
            l.copy_node(c, new);
            l.static_selects.insert(c);
            changed = true;
        }
        if changed {
            l.recopy(parent);
        }
    }
    let mut lifted = FxHashSet::default();
    for t in subtrees(l.tpd, td).into_iter().rev() {
        normalize(l, t, &mut lifted);
    }
}

/// Makes `{ qual; g }(args)` `{ qual; g(args) }` for a block `transform` made, or one in
/// `lifted`, which then holds `t` (dotty's `SelectStatic.normalize`).
pub fn normalize(l: &mut Lower, t: TNodeId, lifted: &mut FxHashSet<TNodeId>) {
    let (Tree::Apply { fun, args } | Tree::TypeApply { fun, args }) = *l.tpd.tree(t) else { return };
    let Tree::Block { stats, expr } = *l.tpd.tree(fun) else { return };
    if !l.static_selects.contains(&fun) && !lifted.contains(&fun) {
        return;
    }
    let tree = match l.tpd.tree(t) {
        Tree::Apply { .. } => Tree::Apply { fun: expr, args },
        _ => Tree::TypeApply { fun: expr, args },
    };
    let (tpe, sym) = (l.tpd.get(t).tpe, l.tpd.get(t).sym);
    let applied = l.tpd.node(tree, Span::NONE, tpe, sym);
    let applied = l.positioned(applied);
    let block = l.tpd.node(Tree::Block { stats, expr: applied }, Span::NONE, tpe, None);
    let block = l.positioned(block);
    l.copy_node(t, block);
    lifted.insert(t);
}

/// The span of a reference in place of `c`, the right side of field or getter `parent` of a
/// class, as dotty's `Constructors` leaves it moved into the constructor before `SelectStatic`:
/// the start of `c`, of `parent` in a trait.
fn moved_init_span(l: &Lower, parent: TNodeId, c: TNodeId) -> Option<Span> {
    let (Tree::ValDef { rhs: Some(rhs), .. } | Tree::DefDef { rhs: Some(rhs), .. }) = *l.tpd.tree(parent)
    else {
        return None;
    };
    let s = l.tpd.sym(parent)?;
    let owner = l.u.symbol(s).owner?;
    let field = match l.tpd.tree(parent) {
        Tree::ValDef { .. } => true,
        _ => has(l.u, s, FIELDaccessor),
    };
    if rhs != c || !field || has(l.u, s, LAZY) || l.u.symbol(owner).kind != Kind::Class {
        return None;
    }
    let span = if l.u.is_trait(owner) { l.tpd.span(parent) } else { l.tpd.span(c) };
    span.exists().then(|| span.start_pos())
}

/// Gives the constructor of superclass call `call` the call's span (dotty's `Mixin.superRef`).
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

/// Whether `f` is an `@static` member moved to its class (dotty's `isScalaStatic`).
pub fn is_scala_static(l: &mut Lower, f: SymbolId) -> bool {
    has(l.u, f, STATIC) && l.u.is_annotated(f, "scala.annotation.static")
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
