// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Getters.scala
//! dotty's `Getters` on assignments: an assignment to a variable but an extern one becomes a call
//! of its setter.

use super::getters::is_setter;
use crate::symbols::{has, is_extern};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, MUTABLE, TRAIT};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Makes each assignment to a variable in class definition `td` a call of its setter.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        todo.extend(super::walk::children(l.tpd, t));
        let Tree::Assign { lhs, rhs } = *l.tpd.tree(t) else { continue };
        if let Some(trait_) = l.tpd.sym(lhs).and_then(|s| library_trait_var(l, s)) {
            super::library_traits::prepare(l, trait_, l.tpd.span(t))?;
        }
        let var = l.tpd.sym(lhs).filter(|&s| has(l.u, s, FIELDaccessor) || is_library_var_param(l, s));
        let Some(var) = var.filter(|&s| !is_extern(l.u, s)) else {
            continue;
        };
        let owner = l.u.symbol(var).owner.expect("a variable's class");
        let setter_name = format!("{}_=", l.u.symbol(var).name);
        let setter = l.u.decl(owner, &setter_name).unwrap_or_default().into_iter().find(|&s| is_setter(l, s));
        let span = l.tpd.span(t);
        let Some(setter) = setter else {
            return unsupported(span, "an assignment to a value without a setter");
        };
        let fun = match *l.tpd.tree(lhs) {
            Tree::Select { qualifier, .. } => l.select(qualifier, setter, l.tpd.span(lhs)),
            _ => {
                let s = l.tpd.span(rhs);
                l.ident(setter, if s.exists() { s.with_point(s.start()) } else { l.tpd.span(lhs) })
            }
        };
        let args = l.tpd.list(&[rhs]);
        l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    }
    Ok(())
}

/// Whether `s` is a `var` parameter or member of a class as a library class declares it, which
/// a library inline body assigns.
fn is_library_var_param(l: &Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    let param = sym.kind == Kind::Param && sym.owner.is_some_and(|o| l.u.is_class(o));
    let member = sym.kind == Kind::Val
        && sym.tree.is_some()
        && sym.owner.is_some_and(|o| l.u.is_class(o) && !has(l.u, o, TRAIT));
    (param || member) && sym.flags.has(MUTABLE)
}

/// The library trait whose `var` `s` is, if it is one.
fn library_trait_var(l: &Lower, s: SymbolId) -> Option<SymbolId> {
    let sym = l.u.symbol(s);
    let owner = sym.owner.filter(|&o| l.u.symbol(o).tree.is_some() && has(l.u, o, TRAIT))?;
    (sym.kind == Kind::Val && sym.flags.has(MUTABLE)).then_some(owner)
}
