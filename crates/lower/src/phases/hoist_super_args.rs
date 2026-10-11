//! dotty's `HoistSuperArgs` for top-level classes: a superclass constructor argument defining
//! methods or classes or creating instances of classes that are not static becomes a private
//! static method `C$superArg$N` of the class taking the constructor's parameters, called in its
//! place.

use super::lambda_lift::fresh;
use super::walk::subtrees;
use crate::definitions::{symbol, type_ref};
use crate::symbols::is_static;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{PRIVATE, STATIC, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return Ok(()) };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let Some(&parent) = l.tpd.items(parents).first() else { return Ok(()) };
    let mut calls = Vec::new();
    let mut call = parent;
    while let Tree::Apply { fun, .. } = *l.tpd.tree(call) {
        calls.push(call);
        call = fun;
    }
    let mut hoisted = Vec::new();
    for &call in calls.iter().rev() {
        let Tree::Apply { fun, args } = *l.tpd.tree(call) else { unreachable!("an application") };
        let mut items = l.tpd.items(args).to_vec();
        let mut end = l.tpd.span(fun).end();
        let mut changed = false;
        for arg in &mut items {
            if needs_hoist(l, *arg) {
                let (def, call) = hoist(l, class, constr, *arg, Span::at(end));
                hoisted.push(def);
                *arg = call;
                changed = true;
            } else if l.tpd.span(*arg).exists() {
                end = end.max(l.tpd.span(*arg).end());
            }
        }
        if changed {
            let args = l.tpd.list(&items);
            l.tpd.get_mut(call).tree = Tree::Apply { fun, args };
        }
    }
    if !hoisted.is_empty() {
        let stats: Vec<TNodeId> = l.tpd.items(body).iter().copied().chain(hoisted).collect();
        let body = l.tpd.list(&stats);
        l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    }
    Ok(())
}

/// Whether superclass constructor argument `arg` defines methods or classes or creates instances
/// of classes that are not static (dotty's `needsHoist`).
pub fn needs_hoist(l: &mut Lower, arg: TNodeId) -> bool {
    subtrees(l.tpd, arg).into_iter().any(|t| match *l.tpd.tree(t) {
        Tree::DefDef { .. } | Tree::Template { .. } => true,
        Tree::New { tpt } => {
            let tpe = l.tpd.tpe(tpt);
            l.u.class_symbol(tpe).is_some_and(|c| !is_static(l.u, c))
        }
        _ => false,
    })
}

/// The definition of the method `class` gets for argument `arg` of the superclass constructor
/// call in constructor definition `constr`, at the constructor if it has value or type parameters,
/// else at the argument, and the call of it with the constructor's parameters, at `span`.
fn hoist(l: &mut Lower, class: SymbolId, constr: TNodeId, arg: TNodeId, span: Span) -> (TNodeId, TNodeId) {
    let originals = params(l, constr);
    let name = fresh(l, &format!("{}$superArg", l.u.symbol(class).name));
    let flags = Flags::default().with(PRIVATE).with(SYNTHETIC).with(STATIC);
    let m = l.u.define(symbol(&name, Kind::Def, class, flags));
    let mut params = Vec::new();
    for &p in &originals {
        let name = l.u.symbol(p).name.clone();
        let q = l.u.define(symbol(&name, Kind::Param, m, Flags::default()));
        let info = l.u.info(p);
        l.u.set_info(q, info);
        params.push(q);
    }
    let result = l.u.widen(l.tpd.tpe(arg));
    let typed = params.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p))).collect();
    let binder = l.u.fresh_binder();
    let info =
        l.u.intern_type(Type::Method { binder, params: typed, result, implicit: false, contextual: false });
    l.u.set_info(m, info);
    let at = l.tpd.span(arg);
    let args: Vec<TNodeId> = originals.iter().map(|&p| l.ident(p, span)).collect();
    let this = l.this(class, span);
    let fun = l.select(this, m, span);
    let call = l.apply(fun, &args, result, span);
    let defined: Vec<SymbolId> = subtrees(l.tpd, arg)
        .into_iter()
        .filter(|&t| {
            matches!(l.tpd.tree(t), Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. })
        })
        .filter_map(|t| l.tpd.sym(t))
        .collect();
    for t in subtrees(l.tpd, arg) {
        let Some(s) = l.tpd.sym(t) else { continue };
        let reference = matches!(l.tpd.tree(t), Tree::Ident { .. });
        if let Some(i) = originals.iter().position(|&p| p == s).filter(|_| reference) {
            let tpe = type_ref(l.u, params[i]);
            let node = l.tpd.get_mut(t);
            (node.sym, node.tpe) = (Some(params[i]), Some(tpe));
        } else if defined.contains(&s) && l.u.symbol(s).owner.is_none_or(|o| !defined.contains(&o)) {
            l.u.set_owner(s, m);
        }
    }
    let ctor_at = l.tpd.span(constr);
    let param_defs: Vec<TNodeId> = params.iter().map(|&p| l.param_def(p, ctor_at)).collect();
    let tpt = l.type_tree(result, at);
    let none = params.is_empty() && l.u.type_params(class).is_empty();
    let def = l.method_def(m, &param_defs, tpt, arg, if none { at } else { ctor_at });
    (def, call)
}

/// The value parameters of constructor definition `constr`.
fn params(l: &Lower, constr: TNodeId) -> Vec<SymbolId> {
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(constr) else { return Vec::new() };
    let clauses = l.tpd.clause_items(paramss);
    let defs = clauses.iter().flat_map(|&c| l.tpd.items(c));
    defs.filter(|&&p| matches!(l.tpd.tree(p), Tree::ValDef { .. })).filter_map(|&p| l.tpd.sym(p)).collect()
}
