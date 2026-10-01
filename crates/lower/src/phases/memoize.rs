//! dotty's `Memoize`: each getter of a class gets a private field that its right-hand side
//! initializes and that the getter returns.

use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result};
use tasty::tags::{FIELDaccessor, LOCAL, PRIVATE};
use tasty::{Flags, Kind};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut stats = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        let getter = l.tpd.sym(stat);
        let (Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }, Some(getter)) =
            (l.tpd.tree(stat).clone(), getter)
        else {
            stats.push(stat);
            continue;
        };
        if !has(l.u, getter, FIELDaccessor) {
            stats.push(stat);
            continue;
        }
        let name_text = l.u.symbol(getter).name.clone();
        let flags = Flags::default().with(PRIVATE).with(LOCAL);
        let field = l.u.define(symbol(&name_text, Kind::Val, class, flags));
        l.fields.insert(field);
        let info = l.u.info(getter);
        l.u.set_info(field, info);
        let span = l.tpd.span(stat);
        let field_tpt = l.type_tree(info, span);
        let def = Tree::ValDef { name, tpt: field_tpt, rhs: Some(rhs) };
        stats.push(l.tpd.node(def, span, None, Some(field)));
        let read = l.field_ref(field, l.tpd.span(rhs));
        l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(read) };
        stats.push(stat);
    }
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}
