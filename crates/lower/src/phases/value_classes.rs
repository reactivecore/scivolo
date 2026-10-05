//! Calls of value class methods (dotty's `VCInlineMethods` and the extension methods of
//! `ExtensionMethods`): `q.m(args)` for `m` of value class `C` becomes
//! `C.m$extension(q)(args)`, a method of the companion object taking the value first.

use super::adapt::from_start;
use super::walk::children;
use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_constructor, is_module, is_trait};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FINAL, INLINE, MACRO};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Rewires the calls of value class methods in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        rewire(l, t)?;
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        if !nested {
            todo.extend(children(l.tpd, t));
        }
    }
    Ok(())
}

/// Makes `t`, a call or selection of a value class method, call its extension method.
fn rewire(l: &mut Lower, t: TNodeId) -> Result<()> {
    let mut argss = Vec::new();
    let mut fun = t;
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: f, args } => {
                argss.push(l.tpd.items(args).to_vec());
                fun = f;
            }
            Tree::TypeApply { .. } => return Ok(()),
            _ => break,
        }
    }
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return Ok(()) };
    let Some(m) = l.tpd.sym(fun) else { return Ok(()) };
    if !has_extension(l, m) {
        let receiver = l.u.widen(l.tpd.tpe(qualifier));
        let value_class =
            l.u.class_symbol(receiver).is_some_and(|c| !is_trait(l.u, c) && l.u.underlying(c).is_some());
        if value_class {
            return unsupported(l.tpd.span(t), "members value classes inherit");
        }
        return Ok(());
    }
    let span = from_start(l.tpd.span(t));
    let Some(extension) = extension(l, m) else {
        return unsupported(span, "methods of generic value classes or with type parameters");
    };
    let class = l.u.symbol(m).owner.expect("a method's class");
    let module = companion(l, class).expect("the companion of an extension method's class");
    let module_ref = l.ident(module, span);
    let mut call = l.select(module_ref, extension, span);
    let info = l.u.info(extension);
    let mut tpe = l.u.result_type(info);
    call = l.apply(call, &[qualifier], tpe, span);
    let call_span = if argss.is_empty() { span } else { l.tpd.span(t) };
    for args in argss.into_iter().rev() {
        tpe = l.u.result_type(tpe);
        call = l.apply(call, &args, tpe, call_span);
    }
    l.tpd.get_mut(call).span = call_span;
    let tpt = l.type_tree(tpe, span);
    let node = l.tpd.get_mut(t);
    (node.tree, node.tpe, node.sym) = (Tree::Typed { expr: call, tpt }, Some(tpe), None);
    Ok(())
}

/// Whether method `m` has an extension method (dotty's `isMethodWithExtension`).
fn has_extension(l: &mut Lower, m: SymbolId) -> bool {
    let s = l.u.symbol(m);
    let Some(owner) = s.owner.filter(|_| s.kind == Kind::Def && !s.name.starts_with("super$")) else {
        return false;
    };
    !is_constructor(l.u, m)
        && !has(l.u, m, INLINE)
        && !has(l.u, m, MACRO)
        && !is_trait(l.u, owner)
        && l.u.symbol(owner).kind == Kind::Class
        && l.u.underlying(owner).is_some()
}

/// The extension method `m$extension` of method `m` of a value class, in the companion object's
/// class: of type `($this: C)` then `m`'s; entered once per method, so overloads get their own.
/// `None` for a generic class or method.
fn extension(l: &mut Lower, m: SymbolId) -> Option<SymbolId> {
    let class = l.u.symbol(m).owner?;
    if !l.u.type_params(class).is_empty() {
        return None;
    }
    let info = l.u.info(m);
    if matches!(l.u.types().get(info), Type::Poly { .. }) {
        return None;
    }
    let module = companion(l, class)?;
    let module_class = l.u.module_class(module)?;
    if let Some(&found) = l.extensions.get(&m) {
        return Some(found);
    }
    let name = format!("{}$extension", l.u.symbol(m).name);
    let sym = l.u.define(symbol(&name, Kind::Def, module_class, Flags::default().with(FINAL)));
    l.extensions.insert(m, sym);
    let this = type_ref(l.u, class);
    let binder = l.u.fresh_binder();
    let params = vec![("$this".to_owned(), this)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: info, implicit: false, contextual: false });
    l.u.set_info(sym, info);
    Some(sym)
}

/// The companion object of class `c`.
fn companion(l: &mut Lower, c: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(c).owner?;
    let name = l.u.symbol(c).name.clone();
    l.u.decl(owner, &name).ok()?.into_iter().find(|&s| is_module(l.u, s))
}
