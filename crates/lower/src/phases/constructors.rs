//! The superclass constructor call of dotty's `Mixin` and the constructor body of
//! `Constructors`: that call, then the fields initialized in order, for classes whose templates
//! hold only methods and fields.

use crate::symbols::{is_constructor, is_trait};
use crate::{Lower, Result, unsupported};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, body, .. } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let span = l.tpd.span(td);
    if is_trait(l.u, class) {
        return unsupported(span, "traits");
    }
    let mut inits = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        match *l.tpd.tree(stat) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::ValDef { rhs: None, .. } => {}
            Tree::ValDef { name, tpt, rhs: Some(rhs) } => {
                let field = l.tpd.sym(stat).expect("a field's symbol");
                let at = l.tpd.span(rhs);
                let lhs = l.field_ref(field, at);
                let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
                inits.push(l.tpd.node(Tree::Assign { lhs, rhs }, at, Some(unit), None));
                l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: None };
            }
            _ => return unsupported(l.tpd.span(stat), "statements in a class body"),
        }
    }
    let parents = l.tpd.items(parents).to_vec();
    let Some((&first, mixins)) = parents.split_first() else {
        return unsupported(span, "a class without a superclass");
    };
    if !mixins.is_empty() {
        return unsupported(l.tpd.span(mixins[0]), "trait parents");
    }
    let super_call = super_call(l, class, first)?;
    let Tree::DefDef { name, paramss, tpt, rhs: None } = *l.tpd.tree(constr) else {
        return unsupported(l.tpd.span(constr), "a constructor with a body");
    };
    let unit = l.unit_literal(span);
    let stats: Vec<TNodeId> = std::iter::once(super_call).chain(inits).collect();
    let rhs = l.block(&stats, unit, span);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    Ok(())
}

/// `super.<init>(args)` for parent `new P(args)`.
fn super_call(l: &mut Lower, class: tasty::SymbolId, parent: TNodeId) -> Result<TNodeId> {
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
