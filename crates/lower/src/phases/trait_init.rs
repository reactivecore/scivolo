//! dotty's `Constructors` for a trait with initialization: its constructor `$init$` calls the
//! setter of each value with its initializer and runs the statements of the body, in order.

use super::accessor_names::trait_setter_name;
use super::first_transform::literal_of;
use super::getters::{is_const_final, is_setter};
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a trait definition's symbol");
    if l.u.no_inits(class) {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a trait definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let mut class_stats = Vec::new();
    let mut inits = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        match l.tpd.tree(stat).clone() {
            Tree::Import { .. } => {}
            Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }
                if l.tpd.sym(stat).is_some_and(|s| {
                    has(l.u, s, FIELDaccessor)
                        && !has(l.u, s, LAZY)
                        && !is_setter(l, s)
                        && is_const_final(l, s)
                }) && !matches!(l.tpd.tree(rhs), Tree::Literal { .. } | Tree::ClassLiteral { .. }) =>
            {
                let getter = l.tpd.sym(stat).expect("a getter's symbol");
                let literal = literal_of(l, getter, l.tpd.span(stat)).unwrap_or(rhs);
                if literal != rhs {
                    inits.push(rhs);
                }
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(literal) };
                class_stats.push(stat);
            }
            Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }
                if l.tpd.sym(stat).is_some_and(|s| {
                    has(l.u, s, FIELDaccessor)
                        && !is_setter(l, s)
                        && !is_const_final(l, s)
                        && !has(l.u, s, LAZY)
                }) =>
            {
                let getter = l.tpd.sym(stat).expect("a getter's symbol");
                inits.push(set(l, class, getter, rhs, l.tpd.span(stat)));
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: None };
                class_stats.push(stat);
            }
            Tree::DefDef { .. } | Tree::TypeDef { .. } => class_stats.push(stat),
            _ => inits.push(stat),
        }
    }
    let Tree::DefDef { name, paramss, tpt, .. } = *l.tpd.tree(constr) else { unreachable!("a constructor") };
    let span = inits.iter().map(|&s| l.tpd.span(s)).filter(|s| s.exists()).reduce(|a, b| a.union(b));
    let span = span.map_or(l.tpd.span(constr), |s| s.with_point(s.start()));
    let unit = l.unit_literal(span);
    let rhs = l.block(&inits, unit, span);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    let body = l.tpd.list(&class_stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `this.x_=(rhs)`, or `this.T$_setter_$x_=(rhs)` for a `val`, for getter `getter` of trait `class`,
/// `rhs` at the definition's `span`.
fn set(l: &mut Lower, class: tasty::SymbolId, getter: tasty::SymbolId, rhs: TNodeId, span: Span) -> TNodeId {
    let name = l.u.symbol(getter).name.clone();
    let var_setter =
        l.u.decl(class, &format!("{name}_=")).unwrap_or_default().into_iter().find(|&s| is_setter(l, s));
    let setter = var_setter.or_else(|| {
        let trait_setter = trait_setter_name(l, class, &name);
        l.u.decl(class, &trait_setter).unwrap_or_default().into_iter().next()
    });
    let setter = setter.expect("a value's setter");
    let this = l.this(class, Span::NONE);
    let fun = l.select(this, setter, Span::NONE);
    let unit = l.u.class_ref("scala.Unit");
    l.tpd.get_mut(rhs).span = span;
    let call = l.apply(fun, &[rhs], unit, Span::NONE);
    l.positioned(call)
}
