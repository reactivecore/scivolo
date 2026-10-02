//! dotty's `Getters`: each value of a class becomes a parameterless getter method with the
//! value's right-hand side, none for a class parameter's; a private local value stays a field.

use super::walk::references;
use crate::symbols::{has, is_class, is_module, is_trait};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{FIELDaccessor, FINAL, LAZY, LOCAL, PARAMsetter, PRIVATE};
use tasty::{Erased, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { return Ok(()) };
    let stats = l.tpd.items(body).to_vec();
    let mut in_methods = FxHashSet::default();
    for &stat in &stats {
        if let Tree::DefDef { rhs: Some(rhs), .. } = *l.tpd.tree(stat) {
            in_methods.extend(references(l.tpd, rhs));
        }
    }
    for stat in stats {
        let Tree::ValDef { name, tpt, rhs } = *l.tpd.tree(stat) else { continue };
        let Some(sym) = l.tpd.sym(stat) else { continue };
        let owner = l.u.symbol(sym).owner;
        if is_module(l.u, sym) || !owner.is_some_and(|o| is_class(l.u, o)) {
            continue;
        }
        let span = l.tpd.span(stat);
        if owner.is_some_and(|o| is_trait(l.u, o)) {
            return unsupported(span, "traits with initialization");
        }
        if has(l.u, sym, LAZY) {
            return unsupported(span, "lazy values");
        }
        if rhs.is_none() && !is_param_accessor(l, sym) {
            return unsupported(span, "abstract values");
        }
        let info = l.u.info(sym);
        if matches!(l.u.erase(info), Erased::Unit | Erased::Null | Erased::Nothing) {
            return unsupported(span, "fields of type Unit, Null or Nothing");
        }
        if has(l.u, sym, PRIVATE) && has(l.u, sym, LOCAL) {
            if !in_methods.contains(&sym) {
                return unsupported(span, "private values used only in the constructor");
            }
            l.fields.insert(sym, sym);
            continue;
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

/// Whether `s` is a `final` value of a constant type.
pub fn is_const_final(l: &mut Lower, s: SymbolId) -> bool {
    if !has(l.u, s, FINAL) {
        return false;
    }
    let info = l.u.info(s);
    matches!(l.u.types().get(info), Type::Constant(..) | Type::Str(_))
}

/// Whether `s` is the setter `x_=` of a variable.
pub fn is_setter(l: &Lower, s: SymbolId) -> bool {
    has(l.u, s, FIELDaccessor) && l.u.symbol(s).name.ends_with("_=")
}
