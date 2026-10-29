// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Constructors.scala
//   compiler/src/dotty/tools/dotc/transform/ExplicitOuter.scala
//! The secondary constructors of a class holding its enclosing instance: each takes it as its
//! own leading parameter `$outer`, passes it on in its self-constructor call and reads it for the
//! enclosing instance (dotty's outer parameters of constructors and `Constructors.mapOuter`).

use super::explicit_outer::terms;
use super::leading_params::add_params;
use crate::Lower;
use crate::definitions::symbol;
use syntax::ast::Span;
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Template, Tree};

/// Gives the secondary constructors of class definition `td` of `class` the parameter `$outer`
/// of the type of the primary constructor's, and their self-constructor calls its value.
pub fn declare(l: &mut Lower, td: TNodeId, class: SymbolId) {
    let Template { constr, body, .. } = l.tpd.class_template(td).expect("a class definition");
    let primary = l.tpd.sym(constr);
    let tpe = l.u.info(l.outers[&class].1);
    for def in l.tpd.items(body).to_vec() {
        let Some(ctor) = l.tpd.sym(def).filter(|&s| l.u.is_constructor(s) && Some(s) != primary) else {
            continue;
        };
        if !matches!(l.tpd.tree(def), Tree::DefDef { .. }) {
            continue;
        }
        let param = l.u.define(symbol("$outer", Kind::Param, ctor, Flags::default()));
        l.u.set_info(param, tpe);
        add_params(l, def, ctor, &[param]);
        for t in terms(l, def, false) {
            pass(l, t, class, param);
        }
    }
}

/// Passes `param` first if `t` calls a constructor of `class` on `this`.
fn pass(l: &mut Lower, t: TNodeId, class: SymbolId, param: SymbolId) {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return };
    let callee = super::walk::without_type_args(l.tpd, fun);
    let Tree::Select { qualifier, .. } = *l.tpd.tree(callee) else { return };
    let on_this = matches!(l.tpd.tree(qualifier), Tree::This { .. }) && l.tpd.sym(qualifier) == Some(class);
    let ctor = l.tpd.sym(callee).filter(|&s| l.u.is_constructor(s) && l.u.symbol(s).owner == Some(class));
    if !on_this || ctor.is_none() {
        return;
    }
    let value = l.ident(param, Span::NONE);
    let all: Vec<TNodeId> = std::iter::once(value).chain(l.tpd.items(args).iter().copied()).collect();
    let args = l.tpd.list(&all);
    l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
}

/// The parameter `$outer` of secondary constructor definition `def`, if it has one.
pub fn param(l: &Lower, def: TNodeId) -> Option<SymbolId> {
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(def) else { return None };
    let ctor = l.tpd.sym(def).filter(|&s| l.u.is_constructor(s))?;
    let first = *l.tpd.clause_items(paramss).first()?;
    let p = l.tpd.items(first).first().and_then(|&p| l.tpd.sym(p))?;
    (l.u.symbol(p).name == "$outer" && l.u.symbol(p).owner == Some(ctor)).then_some(p)
}
