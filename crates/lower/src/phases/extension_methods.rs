//! dotty's `ExtensionMethods` with `FullParameterization`: the body of each method of a value
//! class moves to its extension method `m$extension` in the companion object, `this` becoming
//! the parameter `$this`; the method forwards to it.

use super::value_classes::{extension, has_extension};
use super::walk::subtrees;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Moves the method bodies of the value classes among `classes` to their companions.
pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        if l.u.is_trait(class) || l.u.underlying(class).is_none() {
            continue;
        }
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
        let Tree::Template { body, .. } = *l.tpd.tree(template) else { continue };
        let mut defs = Vec::new();
        for stat in l.tpd.items(body).to_vec() {
            let Some(m) = l.tpd.sym(stat).filter(|_| matches!(l.tpd.tree(stat), Tree::DefDef { .. })) else {
                continue;
            };
            if !has_extension(l, m) {
                continue;
            }
            let Some(ext) = extension(l, m) else {
                return unsupported(l.tpd.span(stat), "methods of value classes without companions");
            };
            defs.push(move_body(l, class, stat, ext));
        }
        let Some(module_class) = defs.first().and_then(|&d| l.tpd.sym(d)).and_then(|e| l.u.symbol(e).owner)
        else {
            continue;
        };
        let Some(&companion) = classes.iter().find(|&&c| l.tpd.sym(c) == Some(module_class)) else {
            return unsupported(l.tpd.span(td), "value classes with a companion in another unit");
        };
        append(l, companion, &defs);
    }
    Ok(())
}

/// Appends `defs` to the body of class definition `td`.
fn append(l: &mut Lower, td: TNodeId, defs: &[TNodeId]) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return };
    let mut stats = l.tpd.items(body).to_vec();
    stats.extend_from_slice(defs);
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
}

/// The definition of extension method `ext` of method definition `def` of value class `class`
/// with `def`'s value parameters and body; `def` gets fresh parameters and forwards to `ext`.
fn move_body(l: &mut Lower, class: SymbolId, def: TNodeId, ext: SymbolId) -> TNodeId {
    let m = l.tpd.sym(def).expect("a method definition's symbol");
    let Tree::DefDef { name, paramss, tpt, rhs } = *l.tpd.tree(def) else { unreachable!("a method") };
    let rhs = rhs.expect("a method with a body");
    let this = l.u.define(symbol("$this", Kind::Param, ext, Flags::default()));
    let class_type = type_ref(l.u, class);
    l.u.set_info(this, class_type);
    let span = l.tpd.span(def);
    let this_def = l.param_def(this, Span::NONE);
    let mut ext_clauses = vec![l.tpd.list(&[this_def])];
    let mut kept = Vec::new();
    let mut forwarded = Vec::new();
    for clause in l.tpd.clause_items(paramss).to_vec() {
        let items = l.tpd.items(clause).to_vec();
        if !items.iter().any(|&p| matches!(l.tpd.tree(p), Tree::ValDef { .. })) {
            kept.push(clause);
            continue;
        }
        ext_clauses.push(clause);
        let originals: Vec<SymbolId> = items.iter().filter_map(|&p| l.tpd.sym(p)).collect();
        let fresh: Vec<SymbolId> = originals.into_iter().map(|p| fresh_param(l, p, m)).collect();
        let defs: Vec<TNodeId> = fresh.iter().map(|&p| l.param_def(p, l.tpd.span(items[0]))).collect();
        kept.push(l.tpd.list(&defs));
        forwarded.push(fresh);
    }
    rewire_body(l, class, m, ext, this, rhs);
    for &clause in &ext_clauses[1..] {
        for p in l.tpd.items(clause).to_vec().into_iter().filter_map(|p| l.tpd.sym(p)) {
            l.u.set_owner(p, ext);
        }
    }
    let rhs_span = l.tpd.span(rhs);
    let call = forwarder(l, class, ext, &forwarded, rhs_span);
    let kept = l.tpd.clauses(&kept);
    l.tpd.get_mut(def).tree = Tree::DefDef { name, paramss: kept, tpt, rhs: Some(call) };
    let ext_name = l.build().term_name(ext);
    let ext_paramss = l.tpd.clauses(&ext_clauses);
    let ext_tpt = l.type_tree(l.tpd.tpe(tpt), l.tpd.span(tpt));
    let tree = Tree::DefDef { name: ext_name, paramss: ext_paramss, tpt: ext_tpt, rhs: Some(rhs) };
    let info = l.u.info(ext);
    l.tpd.node(tree, span, Some(info), Some(ext))
}

/// A parameter of method `m` like `p`.
fn fresh_param(l: &mut Lower, p: SymbolId, m: SymbolId) -> SymbolId {
    let s = l.u.symbol(p);
    let (name, flags) = (s.name.clone(), s.flags);
    let fresh = l.u.define(symbol(&name, Kind::Param, m, flags));
    let info = l.u.info(p);
    l.u.set_info(fresh, info);
    fresh
}

/// Makes body `rhs` of method `m` of value class `class` the body of its extension method `ext`:
/// `this` and the members of `class` reached through it as `$this`, the definitions and returns
/// of `m` those of `ext`.
fn rewire_body(l: &mut Lower, class: SymbolId, m: SymbolId, ext: SymbolId, this: SymbolId, rhs: TNodeId) {
    let module_class = l.u.symbol(ext).owner.expect("an extension method's class");
    for t in subtrees(l.tpd, rhs) {
        let span = l.tpd.span(t);
        let sym = l.tpd.sym(t);
        let rewired = sym.filter(|&s| l.u.symbol(s).owner == Some(class) && has_extension(l, s));
        match (l.tpd.tree(t), rewired) {
            (Tree::This { .. }, _) if sym == Some(class) => {
                let this_ref = l.ident(this, span);
                l.copy_node(t, this_ref);
            }
            (&Tree::Select { qualifier, .. }, Some(method)) => {
                let call = rewired_call(l, module_class, method, qualifier);
                l.copy_node(t, call);
            }
            (Tree::Ident { .. }, Some(method)) => {
                let qualifier = l.ident(this, Span::NONE);
                let call = rewired_call(l, module_class, method, qualifier);
                l.copy_node(t, call);
            }
            (&Tree::Ident { name }, None) if sym.is_some_and(|s| is_member(l, class, s)) => {
                let qualifier = l.ident(this, span);
                l.tpd.get_mut(t).tree = Tree::Select { qualifier, name };
            }
            (&Tree::Return { from, .. }, _) if l.tpd.sym(from) == Some(m) => {
                let to = l.ident(ext, l.tpd.span(from));
                l.copy_node(from, to);
            }
            _ => {}
        }
        if let Some(s) = sym.filter(|&s| l.u.symbol(s).owner == Some(m) && defines(l, t)) {
            l.u.set_owner(s, ext);
        }
    }
}

/// `M.this.ext(qualifier)` for the extension method `ext` of method `m` in object class `module`
/// (`FullParameterization`'s `rewireCall`).
fn rewired_call(l: &mut Lower, module: SymbolId, m: SymbolId, qualifier: TNodeId) -> TNodeId {
    let ext = extension(l, m).expect("the extension method of a value class method");
    let this = l.this(module, Span::NONE);
    let fun = l.select(this, ext, Span::NONE);
    let mut info = l.u.info(ext);
    if let Type::Poly { result, .. } = *l.u.types().get(info) {
        info = result;
    }
    let tpe = l.u.result_type(info);
    let span = l.tpd.span(qualifier);
    l.apply(fun, &[qualifier], tpe, span)
}

/// Whether `s` is a term member of `class`.
fn is_member(l: &Lower, class: SymbolId, s: SymbolId) -> bool {
    l.u.symbol(s).owner == Some(class) && l.u.symbol(s).kind.is_term() && !l.u.is_module(s)
}

/// Whether `t` defines its symbol.
fn defines(l: &Lower, t: TNodeId) -> bool {
    matches!(
        l.tpd.tree(t),
        Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Bind { .. }
    )
}

/// `C.ext(this)(params...)` for the companion `C` of `class`, at `span`.
fn forwarder(l: &mut Lower, class: SymbolId, ext: SymbolId, params: &[Vec<SymbolId>], span: Span) -> TNodeId {
    let module = l.u.companion_module(class).expect("a value class's companion");
    let module_ref = l.ident(module, Span::NONE);
    let mut call = l.select(module_ref, ext, Span::NONE);
    let mut tpe = l.u.info(ext);
    if let Type::Poly { result, .. } = *l.u.types().get(tpe) {
        tpe = result;
    }
    let this = l.this(class, Span::NONE);
    tpe = l.u.result_type(tpe);
    call = l.apply(call, &[this], tpe, Span::NONE);
    for clause in params {
        let args: Vec<TNodeId> = clause.iter().map(|&p| l.ident(p, Span::NONE)).collect();
        tpe = l.u.result_type(tpe);
        call = l.apply(call, &args, tpe, Span::NONE);
    }
    l.tpd.get_mut(call).span = span;
    call
}
