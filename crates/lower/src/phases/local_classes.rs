//! Classes defined in blocks (dotty's `LambdaLift` for classes): each becomes a class of its
//! package named `Outer$Name$N`, taking the local values it uses as its first constructor
//! parameters, kept in private fields where its methods read them.

use super::local_free::{Free, enclosing_class, free_values, is_method, is_term};
use super::local_lift::{lift, pass_values};
use crate::definitions::symbol;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{LOCAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Lifts the local classes among `classes` out of the blocks of unit `root`.
pub fn transform(l: &mut Lower, root: TNodeId, classes: &[TNodeId]) -> Result<()> {
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        let Some(owner) = l.u.symbol(class).owner.filter(|&o| is_term(l, o)) else { continue };
        let outer = enclosing_class(l, owner);
        let free = free_values(l, td, class, outer)?;
        let spans: Vec<Span> = free.in_methods.iter().map(|&v| definition_span(l, root, v)).collect();
        let fields = add_proxies(l, td, class, &free, &spans);
        pass_values(l, root, class, &free, &fields)?;
        lift(l, root, td, class, outer);
    }
    Ok(())
}

/// Adds to class definition `td` a leading constructor parameter for each value of `free` and a
/// private field, at the value's definition span of `spans`, for each its methods use; makes the
/// references to them in its methods read the fields and elsewhere the parameters. The fields.
fn add_proxies(l: &mut Lower, td: TNodeId, class: SymbolId, free: &Free, spans: &[Span]) -> Vec<SymbolId> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let make_fields = |l: &mut Lower| -> Vec<SymbolId> {
        let flags = Flags::default().with(PRIVATE).with(LOCAL);
        free.in_methods.iter().map(|&v| proxy(l, v, class, Kind::Val, flags)).collect()
    };
    let make_params = |l: &mut Lower| -> Vec<SymbolId> {
        free.values.iter().map(|&v| proxy(l, v, ctor, Kind::Param, Flags::default())).collect()
    };
    let (fields, params) = if free.methods_first {
        let fields = make_fields(l);
        (fields, make_params(l))
    } else {
        let params = make_params(l);
        (make_fields(l), params)
    };
    let mut field_defs = Vec::new();
    for ((&v, &field), &at) in free.in_methods.iter().zip(&fields).zip(spans) {
        l.fields.insert(field, field);
        let name = l.term_name(&l.u.symbol(field).name.clone());
        let info = l.u.info(v);
        let tpt = l.type_tree(info, Span::NONE);
        field_defs.push(l.tpd.node(Tree::ValDef { name, tpt, rhs: None }, at, None, Some(field)));
        let i = free.values.iter().position(|&w| w == v).expect("a free value");
        l.proxies.push((field, params[i]));
    }
    add_params(l, constr, ctor, &params);
    for parent in l.tpd.items(parents).to_vec() {
        redirect(l, parent, &free.values, &params, None);
    }
    for stat in l.tpd.items(body).to_vec() {
        match l.tpd.tree(stat) {
            Tree::DefDef { .. } if is_method(l, l.tpd.sym(stat)) => {
                redirect(l, stat, &free.in_methods, &fields, Some(class))
            }
            _ => redirect(l, stat, &free.values, &params, None),
        }
    }
    let stats: Vec<TNodeId> = field_defs.into_iter().chain(l.tpd.items(body).iter().copied()).collect();
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    fields
}

/// Puts `params` first in the first parameter clause of constructor `constr` of `ctor`.
fn add_params(l: &mut Lower, constr: TNodeId, ctor: SymbolId, params: &[SymbolId]) {
    let span = l.tpd.span(constr);
    let param_defs: Vec<TNodeId> = params.iter().map(|&p| l.param_def(p, span)).collect();
    let Tree::DefDef { name, paramss, tpt, rhs } = *l.tpd.tree(constr) else { unreachable!("a constructor") };
    let mut clauses = l.tpd.clause_items(paramss).to_vec();
    match clauses.first_mut() {
        Some(first) => {
            let items: Vec<TNodeId> = param_defs.iter().chain(l.tpd.items(*first)).copied().collect();
            *first = l.tpd.list(&items);
        }
        None => clauses.push(l.tpd.list(&param_defs)),
    }
    let paramss = l.tpd.clauses(&clauses);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs };
    let info = l.u.info(ctor);
    if let Type::Method { binder, params: old, result, implicit, contextual } = l.u.types().get(info).clone()
    {
        let new = params.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p)));
        let params = new.chain(old).collect();
        let info = l.u.intern_type(Type::Method { binder, params, result, implicit, contextual });
        l.u.set_info(ctor, info);
    }
}

/// The span of the definition of local value `v` in unit `root`.
fn definition_span(l: &Lower, root: TNodeId, v: SymbolId) -> Span {
    let def = super::walk::subtrees(l.tpd, root)
        .into_iter()
        .find(|&t| matches!(l.tpd.tree(t), Tree::ValDef { .. }) && l.tpd.sym(t) == Some(v));
    def.map_or(Span::NONE, |d| l.tpd.span(d))
}

/// A symbol `v$N` of `owner` standing for value `v`.
fn proxy(l: &mut Lower, v: SymbolId, owner: SymbolId, kind: Kind, flags: Flags) -> SymbolId {
    let base = format!("{}$", l.u.symbol(v).name);
    let name = l.fresh.next(l.names, &base);
    let name = l.names.text(name).to_owned();
    let p = l.u.define(symbol(&name, kind, owner, flags));
    let info = l.u.info(v);
    l.u.set_info(p, info);
    p
}

/// Makes the references in `t` to values `free` refer to `to`, fields of `this` of `class` if
/// given.
fn redirect(l: &mut Lower, t: TNodeId, free: &[SymbolId], to: &[SymbolId], class: Option<SymbolId>) {
    for s in super::walk::subtrees(l.tpd, t) {
        if !matches!(l.tpd.tree(s), Tree::Ident { .. }) {
            continue;
        }
        let Some(i) = l.tpd.sym(s).and_then(|v| free.iter().position(|&f| f == v)) else { continue };
        let span = l.tpd.span(s);
        let node = match class {
            Some(c) => {
                let this = l.this(c, span);
                l.select(this, to[i], span)
            }
            None => l.ident(to[i], span),
        };
        l.copy_node(s, node);
    }
}
