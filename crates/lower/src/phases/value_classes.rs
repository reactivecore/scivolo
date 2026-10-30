// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/transform/ExtensionMethods.scala
//   compiler/src/dotty/tools/dotc/transform/VCInlineMethods.scala
//   compiler/src/dotty/tools/dotc/transform/ValueClasses.scala
// ExtensionMethods.scala: Copyright 2005-2013 LAMP/EPFL.
//! Calls of value class methods (dotty's `VCInlineMethods` and the extension methods of
//! `ExtensionMethods`): `q.m(args)` for `m` of value class `C` becomes
//! `C.m$extension(q)(args)`, a method of the companion object taking the value first.

use super::adapt::from_start;
use super::eval_once::{eval_once, is_stable_path};
use super::walk::walk_class_up;
use crate::definitions::{symbol, type_ref};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use std::ops::ControlFlow;
use syntax::ast::Span;
use tasty::tags::{FINAL, INLINE, MACRO, PRIVATE};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree, Walk};

/// Rewires the calls of value class methods in class definition `td`, children before their
/// parents, as dotty's `MegaPhase` transforms.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut result = Ok(());
    let mut enter = |l: &mut Lower, t, _| match *l.tpd.tree(t) {
        Tree::Template { constr, .. } => l.tpd.sym(constr).map_or(Walk::Children, Walk::Within),
        _ => Walk::Children,
    };
    walk_class_up(l, td, &mut enter, &mut |l, t, owner| {
        result = rewire(l, t, owner);
        if result.is_err() { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
    });
    result
}

/// Makes `t`, a call or selection of a value class method in `owner`, call its extension method.
fn rewire(l: &mut Lower, t: TNodeId, owner: SymbolId) -> Result<()> {
    let Some(tpe) = l.tpd.get(t).tpe else { return Ok(()) };
    let widened = l.u.widen(tpe);
    if matches!(l.u.types().get(widened), Type::Method { .. } | Type::Poly { .. }) {
        return Ok(());
    }
    let mut argss = Vec::new();
    let mut fun = t;
    let mut targs_span = Span::NONE;
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: f, args } => {
                argss.push(l.tpd.items(args).to_vec());
                fun = f;
            }
            Tree::TypeApply { fun: f, args } => {
                targs_span = l.tpd.items(args).iter().fold(targs_span, |s, &a| s.union(l.tpd.span(a)));
                fun = f;
            }
            _ => break,
        }
    }
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return Ok(()) };
    let Some(m) = l.tpd.sym(fun).filter(|&m| has_extension(l, m)) else { return Ok(()) };
    let span = from_start(l.tpd.span(t));
    let Some(extension) = extension(l, m) else {
        return unsupported(span, "methods of value classes without companions");
    };
    let class = l.u.symbol(m).owner.expect("a method's class");
    let generic = !l.u.type_params(class).is_empty();
    let (receiver, ev_def) = if generic && !is_stable_path(l, qualifier) {
        let (ev, def) = eval_once(l, qualifier, owner);
        (l.ident(ev, Span::NONE), Some(def))
    } else {
        (qualifier, None)
    };
    let module = l.u.companion_module(class).and_then(|m| l.u.module_class(m));
    let module = module.expect("the companion of an extension method's class");
    let (module_span, span) = match ev_def {
        Some(_) => (Span::NONE, Span::NONE),
        None if targs_span.exists() => (targs_span.to_synthetic(), span),
        None => (from_start(l.tpd.span(fun)), span),
    };
    let module_ref = l.this(module, module_span);
    let mut call = l.select(module_ref, extension, span);
    if ev_def.is_some() {
        l.with_span(call, targs_span.to_synthetic());
    }
    let info = l.u.info(extension);
    let info = l.u.strip_poly(info);
    let mut tpe = l.u.result_type(info);
    call = l.apply(call, &[receiver], tpe, span);
    l.positioned(call);
    let call_span = if argss.first().is_some_and(|a| !a.is_empty()) { l.tpd.span(t) } else { span };
    let applied = !argss.is_empty();
    if !applied && l.u.value_class(widened).is_some() {
        l.tpd.get_mut(t).span = span;
    }
    for args in argss.into_iter().rev() {
        tpe = l.u.result_type(tpe);
        call = l.apply(call, &args, tpe, span);
        l.positioned(call);
    }
    if ev_def.is_none() {
        l.tpd.get_mut(call).span = call_span;
    }
    if generic {
        let tpe = l.u.widen(l.tpd.tpe(t));
        let tree = match ev_def {
            Some(def) => {
                let block = l.block(&[def], call, Span::NONE);
                l.positioned(block);
                l.tpd.tree(block).clone()
            }
            None if tpe != l.tpd.tpe(t) => Tree::Typed { expr: call, tpt: inferred(l, tpe, span) },
            None => l.tpd.tree(call).clone(),
        };
        l.tpd.get_mut(t).span =
            if ev_def.is_none() && tpe == l.tpd.tpe(t) { call_span } else { l.tpd.span(t) };
        let node = l.tpd.get_mut(t);
        (node.tree, node.tpe, node.sym) = (tree, Some(tpe), None);
        return Ok(());
    }
    let tpt = inferred(l, tpe, span);
    let node = l.tpd.get_mut(t);
    (node.tree, node.tpe, node.sym) = (Tree::Typed { expr: call, tpt }, Some(tpe), None);
    if applied {
        node.span = call_span;
    }
    Ok(())
}

/// An inferred type tree of `tpe`.
fn inferred(l: &mut Lower, tpe: tasty::TypeId, span: syntax::ast::Span) -> TNodeId {
    l.tpd.node(Tree::TypeTree { inferred: true }, span, Some(tpe), None)
}

/// Whether method `m` has an extension method (dotty's `isMethodWithExtension`).
pub fn has_extension(l: &mut Lower, m: SymbolId) -> bool {
    let s = l.u.symbol(m);
    let Some(owner) = s.owner.filter(|_| s.kind == Kind::Def && !s.name.starts_with("super$")) else {
        return false;
    };
    !l.u.is_constructor(m)
        && !l.super_accessors.contains_key(&m)
        && !has(l.u, m, INLINE)
        && !has(l.u, m, MACRO)
        && !l.u.is_trait(owner)
        && l.u.symbol(owner).kind == Kind::Class
        && l.u.underlying(owner).is_some()
}

/// The extension method `m$extension` of method `m` of a value class, in the companion object's
/// class: of type `($this: C)` then `m`'s, inside `m`'s type parameters; entered once per method,
/// so overloads get their own.
pub fn extension(l: &mut Lower, m: SymbolId) -> Option<SymbolId> {
    let class = l.u.symbol(m).owner?;
    let info = l.u.info(m);
    let module = l.u.companion_module(class)?;
    let module_class = l.u.module_class(module)?;
    if let Some(&found) = l.extensions.get(&m) {
        return Some(found);
    }
    let name = format!("{}$extension", l.u.symbol(m).name);
    let flags = match l.u.symbol(m).flags.has(PRIVATE) {
        true => Flags::default().with(FINAL).with(PRIVATE),
        false => Flags::default().with(FINAL),
    };
    let sym = l.u.define(symbol(&name, Kind::Def, module_class, flags));
    l.extensions.insert(m, sym);
    let this = type_ref(l.u, class);
    let binder = l.u.fresh_binder();
    let params = vec![("$this".to_owned(), this)];
    let (poly, result) = match l.u.types().get(info).clone() {
        Type::Poly { binder, params, result } => (Some((binder, params)), result),
        Type::ByName(result) => (None, result),
        _ => (None, info),
    };
    let method = Type::Method { binder, params, result, implicit: false, contextual: false };
    let mut info = l.u.intern_type(method);
    if let Some((binder, params)) = poly {
        info = l.u.intern_type(Type::Poly { binder, params, result: info });
    }
    l.u.set_info(sym, info);
    Some(sym)
}
