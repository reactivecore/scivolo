//! The class parameters of field initializers and class body statements, dotty's `Constructors`
//! `intoConstr`: references to their fields become references to the constructor's parameters;
//! a private parameter's field or private local value stays only where `markUsedPrivateSymbols`
//! retains it.

use super::getters::is_param_accessor;
use super::walk::{children, subtrees};
use crate::symbols::{has, is_effectively_final};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{LAZY, LOCAL, MUTABLE, PRIVATE, STATIC};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Makes the references to the parameter fields of class definition `td` in its field
/// initializers, statements and superclass call refer to the constructor's parameters of their
/// names, and those to the private local values it does not retain plain references; the
/// parameter fields and private local values `retained` keeps.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<FxHashSet<SymbolId>> {
    super::beta_reduce::check(l, td)?;
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, body, .. } = *l.tpd.tree(template) else { return Ok(FxHashSet::default()) };
    let stats = l.tpd.items(body).to_vec();
    for &stat in &stats {
        let private_var = |s| is_param_accessor(l, s) && has(l.u, s, MUTABLE) && has(l.u, s, PRIVATE);
        if l.tpd.sym(stat).is_some_and(private_var) {
            return unsupported(l.tpd.span(stat), "private mutable class parameters");
        }
    }
    let values = droppable(l, class, &stats);
    let retained = retained(l, class, &stats, &values);
    let dropped: Vec<SymbolId> = values.iter().map(|&(v, _)| v).filter(|v| !retained.contains(v)).collect();
    let params = constructor_params(l, constr);
    for stat in stats {
        let lazy = l.tpd.sym(stat).is_some_and(|s| has(l.u, s, LAZY));
        let root = match *l.tpd.tree(stat) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::ValDef { rhs: None, .. } => continue,
            Tree::ValDef { .. } if lazy => continue,
            Tree::ValDef { rhs: Some(rhs), .. } => rhs,
            _ => stat,
        };
        for t in subtrees(l.tpd, root) {
            if let Tree::Select { qualifier, .. } = *l.tpd.tree(t)
                && matches!(l.tpd.tree(qualifier), Tree::This { .. })
                && let Some(value) = l.tpd.sym(t).filter(|v| dropped.contains(v))
            {
                let ident = l.ident(value, l.tpd.span(t));
                l.copy_node(t, ident);
            }
            let Some(field) = field_ref(l, class, t).filter(|&f| !has(l.u, f, MUTABLE)) else { continue };
            if matches!(l.tpd.tree(t), Tree::Select { .. }) && !is_effectively_final(l.u, class, field) {
                continue;
            }
            to_param(l, &params, t, field);
        }
    }
    let Tree::Template { parents, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
    for parent in l.tpd.items(parents).to_vec() {
        for t in subtrees(l.tpd, parent) {
            if let Some(field) = field_ref(l, class, t) {
                to_param(l, &params, t, field);
            }
        }
    }
    Ok(retained)
}

/// Makes reference `t` to parameter field `field` refer to the constructor parameter of its name
/// among `params`.
fn to_param(l: &mut Lower, params: &[SymbolId], t: TNodeId, field: SymbolId) {
    let Some(param) = params.iter().copied().find(|&p| l.u.symbol(p).name == l.u.symbol(field).name) else {
        return;
    };
    let ident = l.ident(param, l.tpd.span(t));
    l.copy_node(t, ident);
}

/// The value parameters of constructor `constr`.
pub fn constructor_params(l: &Lower, constr: TNodeId) -> Vec<SymbolId> {
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(constr) else { return Vec::new() };
    l.tpd
        .clause_items(paramss)
        .iter()
        .flat_map(|&c| l.tpd.items(c))
        .filter(|&&p| matches!(l.tpd.tree(p), Tree::ValDef { .. }))
        .filter_map(|&p| l.tpd.sym(p))
        .collect()
}

/// The parameter field of class `class` that `t` refers to on `this`.
fn field_ref(l: &Lower, class: SymbolId, t: TNodeId) -> Option<SymbolId> {
    let field = match *l.tpd.tree(t) {
        Tree::Ident { .. } => l.tpd.sym(t),
        Tree::Select { qualifier, .. } if matches!(l.tpd.tree(qualifier), Tree::This { .. }) => l.tpd.sym(t),
        _ => None,
    };
    field.filter(|&s| l.u.symbol(s).owner == Some(class) && is_param_accessor(l, s))
}

/// The private local values of class `class` among `stats` that dotty's `Constructors` may make
/// values of the constructor, each with the index of its definition.
fn droppable(l: &Lower, class: SymbolId, stats: &[TNodeId]) -> Vec<(SymbolId, usize)> {
    let mut out = Vec::new();
    for (i, &stat) in stats.iter().enumerate() {
        let (Tree::ValDef { rhs: Some(_), .. }, Some(s)) = (l.tpd.tree(stat), l.tpd.sym(stat)) else {
            continue;
        };
        let private_local = has(l.u, s, PRIVATE) && has(l.u, s, LOCAL);
        let excluded =
            has(l.u, s, LAZY) || has(l.u, s, STATIC) || l.u.is_module(s) || is_param_accessor(l, s);
        if private_local && !excluded && !l.u.is_trait(class) {
            out.push((s, i));
        }
    }
    out
}

/// The private parameter fields and local values of class `class` that `stats` refer to outside
/// the constructor, on another object, from a function the constructor cannot pass its
/// parameters, or, for a value, not after its definition (dotty's `markUsedPrivateSymbols`).
fn retained(
    l: &Lower,
    class: SymbolId,
    stats: &[TNodeId],
    values: &[(SymbolId, usize)],
) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    let mut todo: Vec<(TNodeId, Option<SymbolId>, usize)> =
        stats.iter().enumerate().map(|(i, &s)| (s, None, i)).collect();
    while let Some((t, method, at)) = todo.pop() {
        let inner = match l.tpd.tree(t) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } => l.tpd.sym(t),
            Tree::ValDef { .. } if l.tpd.sym(t).is_some_and(|s| has(l.u, s, LAZY)) => l.tpd.sym(t),
            _ => method,
        };
        todo.extend(children(l.tpd, t).into_iter().map(|c| (c, inner, at)));
        let sym = match *l.tpd.tree(t) {
            Tree::Ident { .. } | Tree::Select { .. } => l.tpd.sym(t),
            _ => None,
        };
        let Some(sym) = sym.filter(|&s| l.u.symbol(s).owner == Some(class)) else { continue };
        let value = values.iter().find(|&&(v, _)| v == sym);
        if !is_param_accessor(l, sym) && value.is_none() {
            continue;
        }
        let on_this = matches!(l.tpd.tree(t), Tree::Ident { .. })
            || matches!(l.tpd.tree(t), Tree::Select { qualifier, .. } if matches!(l.tpd.tree(*qualifier), Tree::This { .. }));
        let in_constructor = on_this
            && method.is_none_or(|m| {
                !l.u.is_module_class(class) && !has(l.u, sym, MUTABLE) && in_anonfun(l, class, m)
            });
        let seen = value.is_none_or(|&(_, defined)| defined < at);
        if !in_constructor || !seen || has(l.u, sym, MUTABLE) && value.is_none() {
            out.insert(sym);
        }
    }
    out
}

/// Whether method `m` is a lambda of the constructor of class `class` (dotty's `checkAnonFun`).
fn in_anonfun(l: &Lower, class: SymbolId, m: SymbolId) -> bool {
    if l.u.symbol(m).name != "$anonfun" {
        return false;
    }
    let Some(owner) = l.u.symbol(m).owner else { return false };
    let o = l.u.symbol(owner);
    (o.owner == Some(class) && o.kind == Kind::Val && !has(l.u, owner, LAZY))
        || enclosing_method(l, owner).is_some_and(|e| in_anonfun(l, class, e))
}

/// The method `s` is in or is, none for a class's constructor.
fn enclosing_method(l: &Lower, s: SymbolId) -> Option<SymbolId> {
    match l.u.symbol(s).kind {
        Kind::Def => Some(s),
        Kind::Class | Kind::Package => None,
        _ => enclosing_method(l, l.u.symbol(s).owner?),
    }
}
