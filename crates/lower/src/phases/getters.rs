//! dotty's `Getters`: each value of a class becomes a parameterless getter method with the
//! value's right-hand side, none for a class parameter's.

use crate::symbols::{has, is_class, is_module, is_trait};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, FINAL, LAZY, MUTABLE, PARAMsetter, PRIVATE};
use tasty::{Erased, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { return Ok(()) };
    for stat in l.tpd.items(body).to_vec() {
        let Tree::ValDef { name, tpt, rhs } = *l.tpd.tree(stat) else { continue };
        let Some(sym) = l.tpd.sym(stat) else { continue };
        let owner = l.u.symbol(sym).owner;
        if is_module(l.u, sym) || !owner.is_some_and(|o| is_class(l.u, o)) {
            continue;
        }
        if owner.is_some_and(|o| is_trait(l.u, o)) {
            return unsupported(l.tpd.span(stat), "traits with initialization");
        }
        if has(l.u, sym, MUTABLE) {
            return unsupported(l.tpd.span(stat), "mutable fields");
        }
        if has(l.u, sym, LAZY) {
            return unsupported(l.tpd.span(stat), "lazy values");
        }
        if rhs.is_none() && !is_param_accessor(l, sym) {
            return unsupported(l.tpd.span(stat), "abstract values");
        }
        if has(l.u, sym, PRIVATE) {
            return unsupported(l.tpd.span(stat), "private fields");
        }
        let info = l.u.info(sym);
        if has(l.u, sym, FINAL) && matches!(l.u.types().get(info), Type::Constant(..) | Type::Str(_)) {
            return unsupported(l.tpd.span(stat), "constant final values");
        }
        if matches!(l.u.erase(info), Erased::Unit | Erased::Null | Erased::Nothing) {
            return unsupported(l.tpd.span(stat), "fields of type Unit, Null or Nothing");
        }
        let flags = l.u.symbol(sym).flags.with(FIELDaccessor);
        l.u.set_flags(sym, flags);
        let paramss = l.tpd.clauses(&[]);
        l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs };
    }
    Ok(())
}

/// Whether `s` is a value a class parameter defines (the typer marks it `PARAMsetter`).
pub fn is_param_accessor(l: &Lower, s: SymbolId) -> bool {
    has(l.u, s, PARAMsetter)
}
