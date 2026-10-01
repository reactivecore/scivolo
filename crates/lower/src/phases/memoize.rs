//! dotty's `Memoize`: each getter of a class gets a private field that its right-hand side
//! (or, for a class parameter, the constructor) initializes and that the getter returns; a
//! setter stores its argument in the field; a constant `final` value needs no field.

use super::getters::{is_const_final, is_setter};
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result};
use tasty::tags::{FIELDaccessor, LOCAL, MUTABLE, PRIVATE};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut stats = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        let accessor = l.tpd.sym(stat).filter(|&s| has(l.u, s, FIELDaccessor));
        let (Tree::DefDef { name, paramss, tpt, rhs }, Some(accessor)) = (l.tpd.tree(stat).clone(), accessor)
        else {
            stats.push(stat);
            continue;
        };
        if is_setter(l, accessor) {
            let rhs = setter_rhs(l, class, accessor, paramss, rhs);
            l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
            stats.push(stat);
            continue;
        }
        if is_const_final(l, accessor) {
            stats.push(stat);
            continue;
        }
        let name_text = l.u.symbol(accessor).name.clone();
        let mut flags = Flags::default().with(PRIVATE).with(LOCAL);
        if has(l.u, accessor, MUTABLE) {
            flags = flags.with(MUTABLE);
        }
        let field = l.u.define(symbol(&name_text, Kind::Val, class, flags));
        l.fields.insert(field, accessor);
        let info = l.u.info(accessor);
        l.u.set_info(field, info);
        let span = l.tpd.span(stat);
        let field_tpt = l.type_tree(info, span);
        let def = Tree::ValDef { name, tpt: field_tpt, rhs };
        stats.push(l.tpd.node(def, span, None, Some(field)));
        let read_span = match rhs {
            Some(r) => l.tpd.span(r),
            None if span.exists() => span.with_point(span.start()),
            None => span,
        };
        let read = l.field_ref(field, read_span);
        l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(read) };
        stats.push(stat);
    }
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `field = x$1` for setter `setter` of class `class`.
fn setter_rhs(
    l: &mut Lower,
    class: SymbolId,
    setter: SymbolId,
    paramss: typer::tpd::Clauses,
    rhs: Option<TNodeId>,
) -> TNodeId {
    let getter_name = l.u.symbol(setter).name.trim_end_matches("_=").to_owned();
    let field = l.fields.iter().find_map(|(&f, &g)| {
        let s = l.u.symbol(g);
        (s.name == getter_name && s.owner == Some(class) && f != g).then_some(f)
    });
    let field = field.expect("a variable's field");
    let param_tree = l.tpd.items(l.tpd.clause_items(paramss)[0])[0];
    let span = rhs.map_or(l.tpd.span(param_tree), |r| l.tpd.span(r));
    let param = l.tpd.sym(param_tree).expect("a setter's parameter");
    let lhs = l.field_ref(field, span);
    let value = l.ident(param, span);
    let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
    l.tpd.node(Tree::Assign { lhs, rhs: value }, span, Some(unit), None)
}
