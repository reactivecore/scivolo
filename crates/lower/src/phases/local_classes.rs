//! Classes defined in blocks (dotty's `LambdaLift` for classes): each becomes a class of its
//! package named `Outer$Name$N`, taking the local values it uses as its first constructor
//! parameters, kept in private fields where its methods read them.

use super::lift_names::{Caller, LocalClass, Pending};
use super::local_free::{Free, enclosing_class, free_values, is_method, is_term};
use super::local_lift::{lift, pass_values};
use super::local_proxies::make_proxies;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Lifts the local classes among `classes` out of the blocks of unit `root`; their names and
/// those of their proxies are left to `lambda_lift`.
pub fn transform(l: &mut Lower, root: TNodeId, classes: &[TNodeId]) -> Result<Pending> {
    let mut locals = Vec::new();
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        let Some(owner) = l.u.symbol(class).owner.filter(|&o| is_term(l, o)) else { continue };
        let outer = enclosing_class(l, owner);
        let free = free_values(l, td, class, outer)?;
        locals.push((td, class, outer, free));
    }
    let added = super::class_deps::close(l, root, &mut locals)?;
    let order = super::class_deps::definition_order(l, &locals);
    let mut pending = Pending::default();
    let proxies = make_proxies(l, &locals, &added, &mut pending);
    for ((_, class, _, free), (fields, _)) in locals.iter().zip(&proxies) {
        pass_values(l, root, *class, free, fields, &mut pending.passed)?;
    }
    let mut classes = Vec::new();
    for ((td, class, outer, free), (fields, params)) in locals.into_iter().zip(proxies) {
        let spans: Vec<Span> = free.in_methods.iter().map(|&v| definition_span(l, root, v)).collect();
        add_proxies(l, td, class, &free, (&fields, &params), &spans);
        if free.called.iter().any(|&(f, _)| l.u.symbol(f).kind == Kind::Def) {
            let fields = free.in_methods.iter().copied().zip(fields).collect();
            let params = free.values.iter().copied().zip(params).collect();
            pending.callers.push(Caller { td, class, fields, params });
        }
        if let Some((block, index)) = lift(l, root, td, class) {
            pending.definitions.push((block, index, free.values.clone()));
        }
        classes.push(LocalClass { at: l.tpd.span(td).start(), class, outer });
    }
    let mut last = None;
    for i in order {
        let c = &classes[i];
        let at = match last {
            Some(before) if c.at < before => before,
            _ => c.at,
        };
        last = Some(at);
        pending.classes.push(LocalClass { at, class: c.class, outer: c.outer });
    }
    Ok(pending)
}

/// Adds to class definition `td` a leading constructor parameter of `params` for each value of
/// `free` and a private field of `fields`, at the value's definition span of `spans`, for each its
/// methods use; makes the references to them in its methods read the fields and elsewhere the
/// parameters.
fn add_proxies(
    l: &mut Lower,
    td: TNodeId,
    class: SymbolId,
    free: &Free,
    (fields, params): (&[SymbolId], &[SymbolId]),
    spans: &[Span],
) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let mut field_defs = Vec::new();
    for ((&v, &field), &at) in free.in_methods.iter().zip(fields).zip(spans) {
        l.fields.insert(field, field);
        let info = l.u.info(v);
        let tpt = l.type_tree(info, Span::NONE);
        field_defs.push(l.build().val_def(field, tpt, None, at));
        let i = free.values.iter().position(|&w| w == v).expect("a free value");
        l.proxies.push((field, params[i]));
    }
    add_params(l, constr, ctor, params);
    for parent in l.tpd.items(parents).to_vec() {
        redirect(l, parent, &free.values, params, None);
    }
    for stat in l.tpd.items(body).to_vec() {
        match l.tpd.tree(stat) {
            Tree::DefDef { .. } if is_method(l, l.tpd.sym(stat)) => {
                redirect(l, stat, &free.in_methods, fields, Some(class))
            }
            _ => redirect(l, stat, &free.values, params, None),
        }
    }
    let stats: Vec<TNodeId> = field_defs.into_iter().chain(l.tpd.items(body).iter().copied()).collect();
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
}

/// Puts `params` first in the first parameter clause of constructor `constr` of `ctor`.
pub fn add_params(l: &mut Lower, constr: TNodeId, ctor: SymbolId, params: &[SymbolId]) {
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
    let info = with_leading_params(l, info, params);
    l.u.set_info(ctor, info);
}

/// Method type `info`, also under type parameters, with `params` first in its first clause.
fn with_leading_params(l: &mut Lower, info: tasty::TypeId, params: &[SymbolId]) -> tasty::TypeId {
    match l.u.types().get(info).clone() {
        Type::Poly { binder, params: tparams, result } => {
            let result = with_leading_params(l, result, params);
            l.u.intern_type(Type::Poly { binder, params: tparams, result })
        }
        Type::Method { binder, params: old, result, implicit, contextual } => {
            let new = params.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p)));
            let params = new.chain(old).collect();
            l.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
        }
        _ => info,
    }
}

/// The span of the definition of local value `v` in unit `root`, none for a parameter of an
/// `apply` that `SpecializeFunctions` copies with new parameters.
fn definition_span(l: &mut Lower, root: TNodeId, v: SymbolId) -> Span {
    let owner = l.u.symbol(v).owner;
    if l.u.symbol(v).kind == Kind::Param && owner.is_some_and(|m| super::specialize::is_specialized(l, m)) {
        return Span::NONE;
    }
    let def = super::walk::subtrees(l.tpd, root)
        .into_iter()
        .find(|&t| matches!(l.tpd.tree(t), Tree::ValDef { .. }) && l.tpd.sym(t) == Some(v));
    def.map_or(Span::NONE, |d| l.tpd.span(d))
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
