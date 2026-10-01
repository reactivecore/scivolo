//! The superclass constructor call of dotty's `Mixin` and the constructor body of
//! `Constructors`: the class parameters stored in their fields, that call, then the fields
//! initialized in order, for classes whose templates hold only methods and fields.

use super::getters::is_param_accessor;
use crate::symbols::{is_constructor, is_trait};
use crate::{Lower, Result, unsupported};
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    if is_trait(l.u, class) {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, body, .. } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let span = l.tpd.span(td);
    let mut inits = Vec::new();
    let mut accessors = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        match *l.tpd.tree(stat) {
            Tree::DefDef { .. } => accessors.extend(l.tpd.sym(stat).filter(|&s| is_param_accessor(l, s))),
            Tree::TypeDef { .. } | Tree::ValDef { rhs: None, .. } => {}
            Tree::ValDef { name, tpt, rhs: Some(rhs) } => {
                let field = l.tpd.sym(stat).expect("a field's symbol");
                let at = l.tpd.span(rhs);
                let lhs = l.field_ref(field, at);
                inits.push(assign(l, lhs, rhs, at));
                l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: None };
            }
            _ => return unsupported(l.tpd.span(stat), "statements in a class body"),
        }
    }
    let Tree::DefDef { name, paramss, tpt, rhs: None } = *l.tpd.tree(constr) else {
        return unsupported(l.tpd.span(constr), "a constructor with a body");
    };
    let params: Vec<SymbolId> = match l.tpd.clause_items(paramss) {
        [] => Vec::new(),
        [first] => l.tpd.items(*first).iter().filter_map(|&p| l.tpd.sym(p)).collect(),
        _ => return unsupported(l.tpd.span(constr), "a constructor with several parameter lists"),
    };
    if accessors.len() != params.len() {
        return unsupported(l.tpd.span(constr), "class parameters without fields");
    }
    let template_span = l.tpd.span(template);
    let mut stats = Vec::new();
    for (accessor, param) in accessors.into_iter().zip(params) {
        let field = l.fields.iter().find(|&(_, &g)| g == accessor).map(|(&f, _)| f);
        let field = field.expect("a class parameter's field");
        let lhs = l.field_ref(field, template_span);
        let value = l.ident(param, template_span);
        stats.push(assign(l, lhs, value, template_span));
    }
    let parents = l.tpd.items(parents).to_vec();
    let Some(&first) = parents.first() else {
        return unsupported(span, "a class without a superclass");
    };
    stats.push(super_call(l, class, first)?);
    stats.extend(inits);
    let block_span = stats.iter().map(|&s| l.tpd.span(s)).filter(|s| s.exists()).reduce(|a, b| a.union(b));
    let block_span = block_span.map_or(span, |s| s.with_point(s.start()));
    let unit = l.unit_literal(block_span);
    let rhs = l.block(&stats, unit, block_span);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    Ok(())
}

fn assign(l: &mut Lower, lhs: TNodeId, rhs: TNodeId, span: syntax::ast::Span) -> TNodeId {
    let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
    l.tpd.node(Tree::Assign { lhs, rhs }, span, Some(unit), None)
}

/// `super.<init>(args)` for parent `new P(args)`.
fn super_call(l: &mut Lower, class: SymbolId, parent: TNodeId) -> Result<TNodeId> {
    let span = l.tpd.span(parent);
    let Tree::Apply { fun, args } = *l.tpd.tree(parent) else {
        return unsupported(span, "a superclass without a constructor call");
    };
    let Some(ctor) = l.tpd.sym(fun).filter(|&s| is_constructor(l.u, s)) else {
        return unsupported(span, "a superclass constructor call with type or several argument lists");
    };
    let parent_type = l.tpd.tpe(parent);
    let sup = l.super_(class, parent_type, span);
    let fun = l.select(sup, ctor, span);
    let args = l.tpd.items(args).to_vec();
    let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
    Ok(l.apply(fun, &args, unit, span))
}
