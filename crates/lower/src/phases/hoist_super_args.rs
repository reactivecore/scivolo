//! dotty's `HoistSuperArgs`: a superclass constructor argument defining methods or classes or
//! creating instances of classes that are not static becomes a private method `C$superArg$N`
//! taking the constructor's parameters and the values the call defines before it, called in its
//! place: a static method of a top-level class, else a method after the class, local or of the
//! enclosing class.

use super::lift_names::fresh;
use super::local_free::is_term;
use super::walk::subtrees;
use crate::definitions::{symbol, type_ref};
use crate::symbols::is_static;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{PRIVATE, STATIC, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// The class whose superclass call is hoisted from, its constructor definition, whether it is
/// local, whether it is not top-level, the methods hoisted so far, where a hoisting involves a
/// method the call defines.
struct Hoister {
    class: SymbolId,
    constr: TNodeId,
    local: bool,
    nested: bool,
    defs: Vec<TNodeId>,
    refused: Option<Span>,
}

pub fn transform(l: &mut Lower, root: TNodeId, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return Ok(()) };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let Some(&parent) = l.tpd.items(parents).first() else { return Ok(()) };
    let owner = l.u.symbol(class).owner.expect("a class's owner");
    let (local, nested) = (is_term(l, owner), l.u.symbol(owner).kind != Kind::Package);
    let mut h = Hoister { class, constr, local, nested, defs: Vec::new(), refused: None };
    from_call(l, &mut h, parent, &mut Vec::new());
    if let Some(span) = h.refused {
        return unsupported(span, "hoisting superclass constructor arguments with methods the call defines");
    }
    if h.defs.is_empty() {
        return Ok(());
    }
    if !nested {
        let stats: Vec<TNodeId> = l.tpd.items(body).iter().copied().chain(h.defs).collect();
        let body = l.tpd.list(&stats);
        l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
        return Ok(());
    }
    for t in subtrees(l.tpd, root) {
        let (Tree::Block { stats, .. } | Tree::Template { body: stats, .. }) = *l.tpd.tree(t) else {
            continue;
        };
        let mut items = l.tpd.items(stats).to_vec();
        let Some(i) = items.iter().position(|&s| s == td) else { continue };
        items.splice(i + 1..i + 1, h.defs);
        let stats = l.tpd.list(&items);
        match l.tpd.tree(t).clone() {
            Tree::Block { expr, .. } => l.tpd.get_mut(t).tree = Tree::Block { stats, expr },
            Tree::Template { constr, parents, self_, .. } => {
                l.tpd.get_mut(t).tree = Tree::Template { constr, parents, self_, body: stats }
            }
            _ => unreachable!("a block or template"),
        }
        break;
    }
    Ok(())
}

/// Hoists the arguments of superclass call `call`, a block defining values `lifted` and those
/// of its statements (dotty's `hoistSuperArgsFromCall`).
fn from_call(l: &mut Lower, h: &mut Hoister, call: TNodeId, lifted: &mut Vec<SymbolId>) {
    match *l.tpd.tree(call) {
        Tree::Block { stats, expr } => {
            for stat in l.tpd.items(stats).to_vec() {
                if let Tree::DefDef { rhs: Some(rhs), .. } = *l.tpd.tree(stat) {
                    if needs_hoist(l, rhs) {
                        h.refused = Some(l.tpd.span(stat));
                    }
                    lifted.extend(l.tpd.sym(stat));
                    continue;
                }
                let Tree::ValDef { name, tpt, rhs: Some(rhs) } = *l.tpd.tree(stat) else { continue };
                if needs_hoist(l, rhs) {
                    let at = Span::at(l.tpd.span(rhs).end());
                    let rhs = hoist(l, h, rhs, lifted, at);
                    l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: Some(rhs) };
                }
                lifted.extend(l.tpd.sym(stat));
            }
            from_call(l, h, expr, lifted);
        }
        Tree::Apply { fun, args } => {
            from_call(l, h, fun, lifted);
            let mut items = l.tpd.items(args).to_vec();
            let mut end = l.tpd.span(fun).end();
            let mut changed = false;
            for arg in &mut items {
                if needs_hoist(l, *arg) {
                    *arg = hoist(l, h, *arg, lifted, Span::at(end));
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
        _ => {}
    }
}

/// Whether superclass constructor argument `arg` defines methods or classes or creates instances
/// of classes that are not static (dotty's `needsHoist`).
fn needs_hoist(l: &mut Lower, arg: TNodeId) -> bool {
    subtrees(l.tpd, arg).into_iter().any(|t| match *l.tpd.tree(t) {
        Tree::DefDef { .. } | Tree::Template { .. } => true,
        Tree::New { tpt } => {
            let tpe = l.tpd.tpe(tpt);
            l.u.class_symbol(tpe).is_some_and(|c| !is_static(l.u, c))
        }
        _ => false,
    })
}

/// Hoists argument `arg` of the superclass constructor call of `h` into a method taking the
/// constructor's parameters and `lifted`, defined at the constructor if it has value or type
/// parameters, else at the argument; the call of it at `span`.
fn hoist(l: &mut Lower, h: &mut Hoister, arg: TNodeId, lifted: &[SymbolId], span: Span) -> TNodeId {
    let (class, constr) = (h.class, h.constr);
    if lifted.iter().any(|&s| l.u.symbol(s).kind == Kind::Def) {
        h.refused = Some(l.tpd.span(arg));
    }
    let mut originals = params(l, constr);
    originals.extend(lifted);
    let name = fresh(l, &format!("{}$superArg", l.u.symbol(class).name));
    let flags = Flags::default().with(PRIVATE).with(SYNTHETIC);
    let (flags, owner) = match h.nested {
        true => (flags, l.u.symbol(class).owner.expect("a nested class's owner")),
        false => (flags.with(STATIC), class),
    };
    let m = l.u.define(symbol(&name, Kind::Def, owner, flags));
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
    let fun = match h.local {
        true => l.ident(m, span),
        false => {
            let this = l.this(owner, span);
            l.select(this, m, span)
        }
    };
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
    l.coords.insert(m, ctor_at);
    let param_defs: Vec<TNodeId> = params.iter().map(|&p| l.param_def(p, ctor_at)).collect();
    let tpt = l.type_tree(result, at);
    let none = params.is_empty() && l.u.type_params(class).is_empty();
    let def = l.method_def(m, &param_defs, tpt, arg, if none { at } else { ctor_at });
    h.defs.push(def);
    call
}

/// The value parameters of constructor definition `constr`.
fn params(l: &Lower, constr: TNodeId) -> Vec<SymbolId> {
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(constr) else { return Vec::new() };
    let clauses = l.tpd.clause_items(paramss);
    let defs = clauses.iter().flat_map(|&c| l.tpd.items(c));
    defs.filter(|&&p| matches!(l.tpd.tree(p), Tree::ValDef { .. })).filter_map(|&p| l.tpd.sym(p)).collect()
}
