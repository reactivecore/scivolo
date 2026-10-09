//! dotty's `Getters`: each value of a class becomes a parameterless getter method with the
//! value's right-hand side, none for a class parameter's; a static or private local value stays a field,
//! none for a class parameter only the constructor uses; an assignment to a variable becomes a
//! call of its setter.

use super::walk::references;
use crate::symbols::{has, is_class, is_module, is_module_class, is_static, is_trait};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{FIELDaccessor, FINAL, LAZY, LOCAL, MUTABLE, PARAMsetter, PRIVATE, STATIC};
use tasty::{Erased, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes the values of class definition `td` getters; drops the private parameter fields not in
/// `retained`; makes the private local values nothing refers to values of the constructor; drops
/// the values of objects in objects.
pub fn transform(l: &mut Lower, td: TNodeId, retained: &FxHashSet<SymbolId>) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, body, .. } = *l.tpd.tree(template) else { return Ok(()) };
    let constructor = l.tpd.sym(constr).expect("a constructor's symbol");
    let stats = l.tpd.items(body).to_vec();
    let (mut in_methods, mut referenced) = (FxHashSet::default(), FxHashSet::default());
    for &stat in &stats {
        if let Tree::DefDef { rhs: Some(rhs), .. } = *l.tpd.tree(stat) {
            in_methods.extend(references(l.tpd, rhs));
        }
        referenced.extend(references(l.tpd, stat));
    }
    let mut kept = Vec::new();
    for stat in stats {
        kept.push(stat);
        let Tree::ValDef { name, tpt, rhs } = *l.tpd.tree(stat) else { continue };
        let Some(sym) = l.tpd.sym(stat) else { continue };
        let owner = l.u.symbol(sym).owner;
        if !owner.is_some_and(|o| is_class(l.u, o)) {
            continue;
        }
        let span = l.tpd.span(stat);
        if is_module(l.u, sym) {
            if !owner.is_some_and(|o| is_module_class(l.u, o) && is_static(l.u, o)) {
                return unsupported(span, "objects in classes and traits");
            }
            kept.pop();
            continue;
        }
        let in_trait = owner.is_some_and(|o| is_trait(l.u, o));
        if in_trait && has(l.u, sym, LAZY) {
            return unsupported(span, "lazy values in traits");
        }
        if in_trait && has(l.u, sym, PRIVATE) {
            return unsupported(span, "private values in traits");
        }
        if has(l.u, sym, LAZY) {
            let flags = l.u.symbol(sym).flags.with(FIELDaccessor);
            l.u.set_flags(sym, flags);
            if let (Some(class), Some(rhs)) = (owner, rhs) {
                super::lazy_vals::define_init(l, class, sym, rhs);
            }
            let paramss = l.tpd.clauses(&[]);
            l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs };
            continue;
        }
        if rhs.is_none() && !is_param_accessor(l, sym) {
            if has(l.u, sym, MUTABLE) {
                return unsupported(span, "abstract variables");
            }
            let flags = l.u.symbol(sym).flags.with(FIELDaccessor);
            l.u.set_flags(sym, flags);
            l.u.set_deferred(sym, true);
            let paramss = l.tpd.clauses(&[]);
            l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: None };
            continue;
        }
        if has(l.u, sym, STATIC) {
            l.fields.insert(sym, sym);
            continue;
        }
        let private_local = has(l.u, sym, PRIVATE) && has(l.u, sym, LOCAL) && !in_trait;
        if private_local && is_param_accessor(l, sym) && !retained.contains(&sym) {
            kept.pop();
            l.u.unenter(sym);
            continue;
        }
        if private_local && !is_param_accessor(l, sym) && !referenced.contains(&sym) {
            l.u.set_owner(sym, constructor);
            continue;
        }
        let info = l.u.info(sym);
        let erased = l.u.erase(info);
        if (private_local || is_param_accessor(l, sym))
            && matches!(erased, Erased::Unit | Erased::Null | Erased::Nothing)
        {
            return unsupported(span, "parameters and private local fields of type Unit, Null or Nothing");
        }
        if erased == Erased::Unit && overrides(l, sym) {
            return unsupported(span, "values of type Unit overriding members");
        }
        if private_local {
            if !is_param_accessor(l, sym) && !in_methods.contains(&sym) {
                return unsupported(span, "private values used only in the constructor");
            }
            l.fields.insert(sym, sym);
            continue;
        }
        let flags = l.u.symbol(sym).flags.with(FIELDaccessor);
        l.u.set_flags(sym, flags);
        if let (Tree::TypeTree { inferred: true }, Some(rhs)) = (l.tpd.tree(tpt), rhs) {
            let at = l.tpd.span(rhs).start_pos();
            l.tpd.get_mut(tpt).span = at;
        }
        let paramss = l.tpd.clauses(&[]);
        l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs };
    }
    let Tree::Template { constr, parents, self_, .. } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let body = l.tpd.list(&kept);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// Makes each assignment to a variable in class definition `td` a call of its setter.
pub fn setter_calls(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        todo.extend(super::walk::children(l.tpd, t));
        let Tree::Assign { lhs, rhs } = *l.tpd.tree(t) else { continue };
        let Some(var) = l.tpd.sym(lhs).filter(|&s| has(l.u, s, FIELDaccessor)) else { continue };
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

/// Records the span of the right-hand side of each getter of class definition `td`, its point at
/// its start.
pub fn record_rhs(l: &mut Lower, td: TNodeId) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { return };
    for &stat in l.tpd.items(body) {
        let (Tree::DefDef { rhs: Some(rhs), .. }, Some(s)) = (l.tpd.tree(stat), l.tpd.sym(stat)) else {
            continue;
        };
        let r = l.tpd.span(*rhs);
        if has(l.u, s, FIELDaccessor) && r.exists() {
            l.getter_rhs.insert(s, r.with_point(r.start()));
        }
    }
}

/// Whether a base class of value `s`'s class has a member of its name.
fn overrides(l: &mut Lower, s: SymbolId) -> bool {
    let Some(class) = l.u.symbol(s).owner else { return false };
    let name = l.u.symbol(s).name.clone();
    let bases = l.u.linearization(class);
    bases[1..].iter().any(|&b| l.u.decl(b, &name).is_ok_and(|d| !d.is_empty()))
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
