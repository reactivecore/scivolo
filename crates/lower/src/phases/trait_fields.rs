//! The trait fields of dotty's `Mixin`: a trait's values become deferred getters, a `val` with a
//! trait setter `T$_setter_$x_=`; a class gets, for each trait it mixes in, a getter of each value
//! it does not override, the call of the trait's initializer `$init$` and its setters, which
//! `Memoize` makes store fields.

use super::accessor_names::{accessor_name, trait_setter_name};
use super::getters::{is_const_final, is_setter};
use super::library_traits::define_setter;
use super::mixin::has_body;
use super::trait_lazies::{lazy_forwarder, module_forwarder};
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE, PRIVATE, SYNTHETIC};
use tasty::{Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Records the unit's traits among `classes` whose bodies have values or statements.
pub fn find_inits(l: &mut Lower, classes: &[TNodeId]) {
    for &td in classes {
        let Some(class) = l.tpd.sym(td).filter(|&c| l.u.is_trait(c)) else { continue };
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *l.tpd.tree(template) else { continue };
        let inits = l.tpd.items(body).iter().any(|&s| match *l.tpd.tree(s) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => false,
            Tree::ValDef { rhs, .. } => {
                let synthetic_module =
                    l.tpd.sym(s).is_some_and(|v| l.u.is_module(v) && has(l.u, v, SYNTHETIC));
                rhs.is_some() && !synthetic_module
            }
            _ => true,
        });
        if inits {
            l.u.set_inits(class);
        }
    }
}

/// Whether `s` is a getter of a concrete value of a trait: a variable's or one with a trait setter.
fn is_trait_getter(l: &mut Lower, s: SymbolId) -> bool {
    if !has(l.u, s, FIELDaccessor) || is_setter(l, s) || has(l.u, s, LAZY) {
        return false;
    }
    let Some(trait_) = l.u.symbol(s).owner else { return false };
    let setter = match has(l.u, s, MUTABLE) {
        true => format!("{}_=", l.u.symbol(s).name),
        false => trait_setter_name(l, trait_, &accessor_name(l, s)),
    };
    l.u.decl(trait_, &setter).is_ok_and(|d| !d.is_empty())
}

/// Makes the getters and setters of trait definition `td` deferred, adding a trait setter for
/// each `val` (dotty's `traitDefs`).
pub fn trait_defs(l: &mut Lower, td: TNodeId) -> Vec<TNodeId> {
    let trait_ = l.tpd.sym(td).expect("a trait definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a trait definition") };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
    let mut out = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        out.push(stat);
        let (Tree::DefDef { name, paramss, tpt, rhs }, Some(s)) = (l.tpd.tree(stat).clone(), l.tpd.sym(stat))
        else {
            continue;
        };
        let abstract_getter = rhs.is_none() && !is_setter(l, s);
        let skip = !has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY) || l.u.is_module(s);
        if skip || is_const_final(l, s) || abstract_getter {
            continue;
        }
        l.u.set_deferred(s, true);
        if is_setter(l, s) {
            l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: None };
            continue;
        }
        let getter_name = l.u.symbol(s).name.clone();
        let has_setter = l.u.decl(trait_, &format!("{getter_name}_=")).is_ok_and(|d| !d.is_empty());
        if !has_setter {
            out.push(trait_setter(l, trait_, s, l.tpd.span(td)));
        }
    }
    out
}

/// The deferred trait setter `T$_setter_$x_=(x$0: X): Unit` of getter `getter` of trait `trait_`.
fn trait_setter(l: &mut Lower, trait_: SymbolId, getter: SymbolId, span: Span) -> TNodeId {
    let name = trait_setter_name(l, trait_, &accessor_name(l, getter));
    let getter_info = l.u.info(getter);
    let value = l.u.result_type(getter_info);
    let setter = define_setter(l, trait_, &name, value);
    let unit = l.u.class_ref("scala.Unit");
    let param = l.u.define(symbol("x$0", Kind::Param, setter, tasty::Flags::default()));
    l.u.set_info(param, value);
    let param = l.param_def(param, span);
    let tpt = l.type_tree(unit, span);
    l.build().def_def(setter, &[&[param]], tpt, None, span)
}

/// The members class `class` gets from trait `mixin` it mixes in (dotty's `traitInits`,
/// `superCallOpt` and `setters`): a getter without a body for each value it does not override,
/// a lazy value for each lazy value it does not override and each object (`trait_lazies.rs`),
/// the call `super[M].$init$()`, a setter with a `()` body for each setter; the call and the
/// bodies at the point of the class's `span`, the members without spans, which copying the
/// template gives them.
pub fn mixed_in(l: &mut Lower, class: SymbolId, mixin: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    let mut out = Vec::new();
    let decls = l.u.decls(mixin).unwrap_or_default();
    for &getter in &decls {
        let library = l.u.symbol(getter).tree.is_some();
        if l.u.is_module(getter) && library {
            return unsupported(span, "objects in library traits");
        }
        if l.u.is_module(getter) {
            out.push(module_forwarder(l, class, getter, span)?);
        } else if has(l.u, getter, LAZY) && has_body(l, getter) && implemented_by(l, class, getter) {
            if library {
                return unsupported(span, "lazy values in library traits");
            }
            let at = if span.exists() { Span::at(span.point()) } else { span };
            out.push(lazy_forwarder(l, class, mixin, getter, at));
        } else if is_trait_getter(l, getter) && !is_const_final(l, getter) && implemented_by(l, class, getter)
        {
            out.push(forwarder(l, class, getter, None));
        }
    }
    let at = if span.exists() { Span::at(span.point()) } else { span };
    if !l.u.no_inits(mixin) {
        let init = l.u.decl(mixin, "<init>").unwrap_or_default();
        let init = init.into_iter().next().expect("a trait constructor");
        let sup = l.super_mix(class, mixin, at);
        let fun = l.select(sup, init, at);
        let unit = l.u.class_ref("scala.Unit");
        let call = l.apply(fun, &[], unit, Span::NONE);
        out.push(l.positioned(call));
    }
    for &setter in &decls {
        if is_setter(l, setter) {
            let unit = l.unit_literal(at);
            out.push(forwarder(l, class, setter, Some(unit)));
        }
    }
    Ok(out)
}

/// Whether the concrete member of `class` named as `getter` is `getter` (dotty's
/// `isInImplementingClass`).
pub fn implemented_by(l: &mut Lower, class: SymbolId, getter: SymbolId) -> bool {
    if has(l.u, getter, PRIVATE) {
        return true;
    }
    let name = l.u.symbol(getter).name.clone();
    for base in l.u.linearization(class) {
        for m in l.u.decl(base, &name).unwrap_or_default() {
            let member = matches!(l.u.symbol(m).kind, Kind::Val | Kind::Def)
                && !has(l.u, m, PRIVATE)
                && !l.bridges.contains(&m);
            if member && (has_body(l, m) || is_trait_getter(l, m)) {
                return m == getter;
            }
        }
    }
    false
}

/// A copy in `class` of accessor `member` with body `rhs` (dotty's `mkForwarderSym`), without a
/// span.
pub fn forwarder(l: &mut Lower, class: SymbolId, member: SymbolId, rhs: Option<TNodeId>) -> TNodeId {
    let span = Span::NONE;
    let name = accessor_name(l, member);
    let m = l.u.symbol(member);
    let (kind, flags) = (m.kind, m.flags.with(SYNTHETIC).without(PRIVATE));
    let target = m.target.clone();
    let sym = l.u.define(symbol(&name, kind, class, flags));
    if let Some(target) = target {
        l.u.set_target(sym, &target);
    }
    l.mixed_in.insert(sym);
    let info = l.u.info(member);
    l.u.set_info(sym, info);
    let params = match l.u.types().get(info).clone() {
        Type::Method { params, .. } => params,
        _ => Vec::new(),
    };
    let defs: Vec<TNodeId> = params
        .into_iter()
        .map(|(n, t)| {
            let p = l.u.define(symbol(&n, Kind::Param, sym, tasty::Flags::default()));
            l.u.set_info(p, t);
            l.param_def(p, span)
        })
        .collect();
    let paramss: &[&[TNodeId]] = if kind == Kind::Def { &[&defs] } else { &[] };
    let result = l.u.result_type(info);
    let tpt = l.type_tree(result, span);
    l.build().def_def(sym, paramss, tpt, rhs, span)
}
