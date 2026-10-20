// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Getters.scala
//! dotty's `Getters`: each value of a class becomes a parameterless getter method with the
//! value's right-hand side, none for a class parameter's; a static or private local value stays a field,
//! none for a class parameter only the constructor uses; an assignment to a variable becomes a
//! call of its setter.

use crate::definitions::symbol;
use crate::symbols::{has, is_static_module};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, FINAL, LAZY, LOCAL, MUTABLE, PARAMsetter, PRIVATE, STATIC, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes the values of class definition `td` getters, an object's lazy ones in a class; drops the
/// private parameter fields not in `retained`; makes the private local values not in `retained`
/// values of the constructor; drops the values of objects in objects.
pub fn transform(l: &mut Lower, td: TNodeId, retained: &FxHashSet<SymbolId>) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, body, .. } = *l.tpd.tree(template) else { return Ok(()) };
    let constructor = l.tpd.sym(constr).expect("a constructor's symbol");
    let stats = l.tpd.items(body).to_vec();
    let mut kept = Vec::new();
    for stat in stats {
        kept.push(stat);
        let Tree::ValDef { name, tpt, rhs } = *l.tpd.tree(stat) else { continue };
        let Some(sym) = l.tpd.sym(stat) else { continue };
        let owner = l.u.symbol(sym).owner;
        if !owner.is_some_and(|o| l.u.is_class(o)) {
            continue;
        }
        let span = l.tpd.span(stat);
        let in_trait = owner.is_some_and(|o| l.u.is_trait(o));
        let in_value_class = owner.is_some_and(|o| l.u.underlying(o).is_some());
        if l.u.is_module(sym) {
            if owner.is_some_and(|o| is_static_module(l.u, o)) {
                kept.pop();
                continue;
            }
            let flags = l.u.symbol(sym).flags.with(LAZY);
            l.u.set_flags(sym, flags);
        }
        if has(l.u, sym, LAZY) {
            let flags = l.u.symbol(sym).flags.with(FIELDaccessor);
            l.u.set_flags(sym, flags);
            if let (Some(class), Some(rhs), false) = (owner, rhs, in_trait) {
                super::lazy_vals::define_members(l, class, sym, rhs);
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
        let private_local = has(l.u, sym, PRIVATE) && has(l.u, sym, LOCAL) && !in_trait && !in_value_class;
        if private_local && is_param_accessor(l, sym) && !retained.contains(&sym) {
            kept.pop();
            l.u.unenter(sym);
            continue;
        }
        if private_local && !is_param_accessor(l, sym) && !retained.contains(&sym) {
            l.u.set_owner(sym, constructor);
            continue;
        }
        if private_local {
            if super::memoize::bottom(l, sym).is_some() {
                return unsupported(span, "private local fields of type Unit, Null or Nothing");
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
        let setter_name = format!("{}_=", l.u.symbol(sym).name);
        let class = owner.expect("a value's class");
        if has(l.u, sym, MUTABLE) && l.u.decl(class, &setter_name).unwrap_or_default().is_empty() {
            let at = Span::at(l.tpd.span(td).point());
            kept.push(param_setter(l, class, sym, &setter_name, at));
        }
    }
    let Tree::Template { constr, parents, self_, .. } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let body = l.tpd.list(&kept);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// The setter `name(x$0)` of variable `var` of class `class` that has none (a private mutable
/// class parameter) at `span`, with all of `var`'s flags and `FIELDaccessor`, its body left to
/// `Memoize` (dotty's `ensureSetter`).
fn param_setter(l: &mut Lower, class: SymbolId, var: SymbolId, name: &str, span: Span) -> TNodeId {
    let flags = l.u.symbol(var).flags.with(FIELDaccessor);
    let setter = l.u.define(symbol(name, Kind::Def, class, flags));
    let value = l.u.info(var);
    let unit = l.u.class_ref("scala.Unit");
    let binder = l.u.fresh_binder();
    let params = vec![("x$0".to_owned(), value)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: unit, implicit: false, contextual: false });
    l.u.set_info(setter, info);
    let param = l.u.define(symbol("x$0", Kind::Param, setter, Flags::default().with(SYNTHETIC)));
    l.u.set_info(param, value);
    let param = l.param_def(param, span);
    let tpt = l.type_tree(unit, span);
    l.build().def_def(setter, &[&[param]], tpt, None, span)
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

/// Whether `s` is a value a class parameter defines (the typer marks it `PARAMsetter`).
pub fn is_param_accessor(l: &Lower, s: SymbolId) -> bool {
    has(l.u, s, PARAMsetter)
}

/// Whether `s` is a `final` value, not a variable, of a constant type.
pub fn is_const_final(l: &mut Lower, s: SymbolId) -> bool {
    let value = l.u.symbol(s).kind == Kind::Val || has(l.u, s, FIELDaccessor);
    if !has(l.u, s, FINAL) || has(l.u, s, MUTABLE) || !value {
        return false;
    }
    let info = l.u.info(s);
    matches!(l.u.types().get(info), Type::Constant(..) | Type::Str(_))
}

/// Whether `s` is the setter `x_=` of a variable.
pub fn is_setter(l: &Lower, s: SymbolId) -> bool {
    has(l.u, s, FIELDaccessor) && l.u.symbol(s).name.ends_with("_=")
}
