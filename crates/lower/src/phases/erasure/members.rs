// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The members of a class definition typed by `Erasure`: methods against their erased result
//! types, statements, field initializers and superclass calls.

use super::typed;
use crate::{Lower, Result};
use tasty::{Erased, Kind};
use typer::tpd::{TNodeId, Tree};

/// Types the methods of class definition `td` against their erased result types, and the
/// statements and field initializers of its body.
pub fn transform_class(l: &mut Lower, td: TNodeId) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut stats = Vec::new();
    let constructor = l.tpd.sym(constr);
    for stat in std::iter::once(constr).chain(l.tpd.items(body).iter().copied()).collect::<Vec<_>>() {
        l.erasure_owner = constructor;
        match *l.tpd.tree(stat) {
            Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) } => {
                let sym = l.tpd.sym(stat).expect("a method definition's symbol");
                l.erasure_owner = Some(sym);
                let pt = if l.u.is_constructor(sym) { Erased::Unit } else { l.u.erased_method_value(sym).1 };
                let before = l.tpd.span(rhs);
                let rhs = typed(l, rhs, Some(&pt))?;
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
                if l.u.symbol(sym).kind == Kind::Val
                    || crate::symbols::has(l.u, sym, tasty::tags::FIELDaccessor)
                {
                    keep_class_literal_start(l, stat, before);
                }
                l.recopy_class_literals(stat);
                stats.push(stat);
            }
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => stats.push(stat),
            _ => stats.push(typed(l, stat, None)?),
        }
    }
    let body = l.tpd.list(&stats[1..]);
    l.erasure_owner = constructor;
    let mut typed_parents = Vec::new();
    for parent in l.tpd.items(parents).to_vec() {
        let call = matches!(l.tpd.tree(parent), Tree::Apply { .. } | Tree::Block { .. });
        typed_parents.push(if call { typed(l, parent, None)? } else { parent });
    }
    let parents = l.tpd.list(&typed_parents);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// Gives the right-hand side of value definition `t` the start of its span `before` erasure if
/// it is a class literal.
pub fn keep_class_literal_start(l: &mut Lower, t: TNodeId, before: syntax::ast::Span) {
    let (Tree::ValDef { rhs: Some(rhs), .. } | Tree::DefDef { rhs: Some(rhs), .. }) = *l.tpd.tree(t) else {
        return;
    };
    if matches!(l.tpd.tree(rhs), Tree::ClassLiteral { .. }) && !l.tpd.span(rhs).exists() && before.exists() {
        l.tpd.get_mut(rhs).span = before.start_pos();
    }
}
