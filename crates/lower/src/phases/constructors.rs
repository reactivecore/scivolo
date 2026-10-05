//! The superclass constructor call of dotty's `Mixin` and the constructor body of
//! `Constructors`: the class parameters stored in their fields, that call, then the field
//! initializers and statements of the class body in order.

use super::first_transform::literal_of;
use super::getters::{is_const_final, is_param_accessor};
use super::walk::references;
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
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let mut class_stats = Vec::new();
    let span = l.tpd.span(td);
    let mut inits = Vec::new();
    let mut accessors = Vec::new();
    let stats = l.tpd.items(body).to_vec();
    for &stat in &stats {
        if matches!(l.tpd.tree(stat), Tree::DefDef { .. } | Tree::ValDef { rhs: None, .. }) {
            accessors.extend(l.tpd.sym(stat).filter(|&s| is_param_accessor(l, s)));
        }
    }
    for stat in stats {
        let sym = l.tpd.sym(stat);
        let const_getter = sym.filter(|&s| is_const_final(l, s));
        match *l.tpd.tree(stat) {
            Tree::Import { .. } => {}
            Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }
                if const_getter.is_some() && !matches!(l.tpd.tree(rhs), Tree::Literal { .. }) =>
            {
                let getter = const_getter.expect("a getter's symbol");
                let literal = literal_of(l, getter, l.tpd.span(stat)).unwrap_or(rhs);
                if literal != rhs {
                    inits.push(rhs);
                }
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(literal) };
                class_stats.push(stat);
            }
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::ValDef { rhs: None, .. } => {
                class_stats.push(stat)
            }
            Tree::ValDef { name, tpt, rhs: Some(rhs) } => {
                class_stats.push(stat);
                let field = l.tpd.sym(stat).expect("a field's symbol");
                let memoized = l.fields.get(&field).is_some_and(|&g| g != field);
                let at = if memoized { l.tpd.span(rhs) } else { l.tpd.span(stat) };
                if references(l.tpd, rhs).iter().any(|s| accessors.contains(s)) {
                    return unsupported(at, "class parameters used in initializers");
                }
                let lhs = l.field_ref(field, at);
                inits.push(assign(l, lhs, rhs, at));
                l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: None };
            }
            _ => {
                if references(l.tpd, stat).iter().any(|s| accessors.contains(s)) {
                    return unsupported(l.tpd.span(stat), "class parameters used in class body statements");
                }
                inits.push(stat);
            }
        }
    }
    let Tree::DefDef { name, paramss, tpt, rhs: None } = *l.tpd.tree(constr) else {
        return unsupported(l.tpd.span(constr), "a constructor with a body");
    };
    let params: Vec<SymbolId> = l
        .tpd
        .clause_items(paramss)
        .iter()
        .flat_map(|&c| l.tpd.items(c))
        .filter(|&&p| matches!(l.tpd.tree(p), Tree::ValDef { .. }))
        .filter_map(|&p| l.tpd.sym(p))
        .collect();
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
    let Some(&first) = l.tpd.items(parents).first() else {
        return unsupported(span, "a class without a superclass");
    };
    stats.push(super_call(l, class, first)?);
    stats.extend(inits);
    let block_span = stats.iter().map(|&s| l.tpd.span(s)).filter(|s| s.exists()).reduce(|a, b| a.union(b));
    let block_span = block_span.map_or(span, |s| s.with_point(s.start()));
    let unit = l.unit_literal(block_span);
    let rhs = l.block(&stats, unit, block_span);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    let body = l.tpd.list(&class_stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
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
