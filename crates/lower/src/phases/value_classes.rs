//! Calls of value class methods (dotty's `VCInlineMethods` and the extension methods of
//! `ExtensionMethods`): `q.m(args)` for `m` of value class `C` becomes
//! `C.m$extension(q)(args)`, a method of the companion object taking the value first.

use super::adapt::from_start;
use super::walk::children;
use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_constructor, is_module, is_trait};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FINAL, INLINE, MACRO, MUTABLE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Rewires the calls of value class methods in class definition `td`, children before their
/// parents, as dotty's `MegaPhase` transforms.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let mut order = Vec::new();
    let mut todo = vec![(td, class)];
    while let Some((t, owner)) = todo.pop() {
        order.push((t, owner));
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        let inner = match l.tpd.tree(t) {
            Tree::DefDef { .. } => l.tpd.sym(t).unwrap_or(owner),
            _ => owner,
        };
        if !nested {
            todo.extend(children(l.tpd, t).into_iter().map(|c| (c, inner)));
        }
    }
    for (t, owner) in order.into_iter().rev() {
        rewire(l, t, owner)?;
    }
    Ok(())
}

/// Makes `t`, a call or selection of a value class method in `owner`, call its extension method.
fn rewire(l: &mut Lower, t: TNodeId, owner: SymbolId) -> Result<()> {
    let Some(tpe) = l.tpd.get(t).tpe else { return Ok(()) };
    let widened = l.u.widen(tpe);
    if matches!(l.u.types().get(widened), Type::Method { .. } | Type::Poly { .. }) {
        return Ok(());
    }
    let mut argss = Vec::new();
    let (mut fun, mut typed) = (t, false);
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: f, args } => {
                argss.push(l.tpd.items(args).to_vec());
                fun = f;
            }
            Tree::TypeApply { fun: f, .. } => (fun, typed) = (f, true),
            _ => break,
        }
    }
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return Ok(()) };
    let Some(m) = l.tpd.sym(fun) else { return Ok(()) };
    if !has_extension(l, m) {
        let receiver = l.u.widen(l.tpd.tpe(qualifier));
        let value_class =
            l.u.class_symbol(receiver).is_some_and(|c| !is_trait(l.u, c) && l.u.underlying(c).is_some());
        if value_class && !typed {
            return unsupported(l.tpd.span(t), "members value classes inherit");
        }
        return Ok(());
    }
    let span = from_start(l.tpd.span(t));
    let Some(extension) = extension(l, m) else {
        return unsupported(span, "methods of value classes without companions");
    };
    let class = l.u.symbol(m).owner.expect("a method's class");
    let generic = !l.u.type_params(class).is_empty();
    let (receiver, ev_def) = if generic && !is_stable_path(l, qualifier) {
        let (ev, def) = eval_once(l, qualifier, owner);
        (l.ident(ev, l.tpd.span(qualifier)), Some(def))
    } else {
        (qualifier, None)
    };
    let module = companion(l, class).expect("the companion of an extension method's class");
    let module_ref = l.ident(module, span);
    let mut call = l.select(module_ref, extension, span);
    let info = l.u.info(extension);
    let info = match l.u.types().get(info).clone() {
        Type::Poly { result, .. } => result,
        _ => info,
    };
    let mut tpe = l.u.result_type(info);
    call = l.apply(call, &[receiver], tpe, span);
    let call_span = if argss.is_empty() { span } else { l.tpd.span(t) };
    for args in argss.into_iter().rev() {
        tpe = l.u.result_type(tpe);
        call = l.apply(call, &args, tpe, call_span);
    }
    l.tpd.get_mut(call).span = call_span;
    if generic {
        let tpe = l.tpd.tpe(t);
        let tree = match ev_def {
            Some(def) => Tree::Block { stats: l.tpd.list(&[def]), expr: call },
            None => l.tpd.tree(call).clone(),
        };
        let node = l.tpd.get_mut(t);
        (node.tree, node.tpe, node.sym) = (tree, Some(tpe), None);
        return Ok(());
    }
    let tpt = l.type_tree(tpe, span);
    let node = l.tpd.get_mut(t);
    (node.tree, node.tpe, node.sym) = (Tree::Typed { expr: call, tpt }, Some(tpe), None);
    Ok(())
}

/// Whether `t` is a literal, `this`, or a path of stable values and objects (dotty's
/// `isIdempotentExpr` for these).
fn is_stable_path(l: &mut Lower, t: TNodeId) -> bool {
    let stable = |l: &mut Lower, s: Option<SymbolId>| {
        s.is_some_and(|s| is_module(l.u, s) || l.u.symbol(s).kind == Kind::Val && !has(l.u, s, MUTABLE))
    };
    match *l.tpd.tree(t) {
        Tree::Literal { .. } | Tree::This { .. } => true,
        Tree::Ident { .. } => stable(l, l.tpd.sym(t)),
        Tree::Select { qualifier, .. } => stable(l, l.tpd.sym(t)) && is_stable_path(l, qualifier),
        _ => false,
    }
}

/// `val ev$N = expr` in `owner` (dotty's `evalOnce`), with its symbol.
fn eval_once(l: &mut Lower, expr: TNodeId, owner: SymbolId) -> (SymbolId, TNodeId) {
    let name = l.fresh.next(l.names, "ev$");
    let name = l.names.text(name).to_owned();
    let ev = l.u.define(symbol(&name, Kind::Val, owner, Flags::default().with(SYNTHETIC)));
    let tpe = l.u.widen(l.tpd.tpe(expr));
    l.u.set_info(ev, tpe);
    let span = l.tpd.span(expr);
    let tpt = l.type_tree(tpe, span);
    let term = l.term_name(&name);
    let def = l.tpd.node(Tree::ValDef { name: term, tpt, rhs: Some(expr) }, span, Some(tpe), Some(ev));
    (ev, def)
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
/// class: of type `($this: C)` then `m`'s, inside `m`'s type parameters; entered once per method,
/// so overloads get their own.
fn extension(l: &mut Lower, m: SymbolId) -> Option<SymbolId> {
    let class = l.u.symbol(m).owner?;
    let info = l.u.info(m);
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
    let (poly, result) = match l.u.types().get(info).clone() {
        Type::Poly { binder, params, result } => (Some((binder, params)), result),
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

/// The companion object of class `c`.
fn companion(l: &mut Lower, c: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(c).owner?;
    let name = l.u.symbol(c).name.clone();
    l.u.decl(owner, &name).ok()?.into_iter().find(|&s| is_module(l.u, s))
}
