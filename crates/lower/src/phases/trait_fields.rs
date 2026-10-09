//! The trait fields of dotty's `Mixin`: a trait's values become deferred getters, a `val` with a
//! trait setter `T$_setter_$x_=`; a class gets, for each trait it mixes in, a getter of each value
//! it does not override, the call of the trait's initializer `$init$` and its setters, which
//! `Memoize` makes store fields.

use super::getters::{is_const_final, is_setter};
use crate::definitions::symbol;
use crate::symbols::{has, is_trait};
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY, PRIVATE, SYNTHETIC};
use tasty::{Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Records the unit's traits among `classes` whose bodies have values or statements.
pub fn find_inits(l: &mut Lower, classes: &[TNodeId]) {
    for &td in classes {
        let Some(class) = l.tpd.sym(td).filter(|&c| is_trait(l.u, c)) else { continue };
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *l.tpd.tree(template) else { continue };
        let inits = l.tpd.items(body).iter().any(|&s| match *l.tpd.tree(s) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => false,
            Tree::ValDef { rhs, .. } => rhs.is_some(),
            _ => true,
        });
        if inits {
            l.u.set_inits(class);
        }
    }
}

/// The separator in the names of trait setters (dotty's `TraitSetterName`).
const TRAIT_SETTER: &str = "$_setter_$";

/// The name of trait `trait_`'s setter of value `x`: `T$_setter_$x_=`, `T` the names of the
/// trait and its owners joined by `$` (dotty's `expandedName`).
pub fn trait_setter_name(l: &Lower, trait_: SymbolId, x: &str) -> String {
    format!("{}{TRAIT_SETTER}{x}_=", expanded_prefix(l, trait_))
}

/// The names of `s` and its owners up to its package, joined by `$`, an object's without its `$`.
fn expanded_prefix(l: &Lower, s: SymbolId) -> String {
    let sym = l.u.symbol(s);
    let name = sym.name.strip_suffix('$').unwrap_or(&sym.name);
    match sym.owner {
        Some(o) if l.u.symbol(o).kind == Kind::Package => match l.u.full_name(o).as_str() {
            "" | "<empty>" => name.to_owned(),
            package => format!("{}${name}", package.replace('.', "$")),
        },
        Some(o) => format!("{}${name}", expanded_prefix(l, o)),
        None => name.to_owned(),
    }
}

/// The name of the getter of setter `x_=` or of trait setter `T$_setter_$x_=`.
pub fn setter_getter_name(setter: &str) -> &str {
    let name = setter.rsplit(TRAIT_SETTER).next().unwrap_or(setter);
    name.strip_suffix("_=").unwrap_or(name)
}

/// Whether `s` is a getter of a value of a trait.
fn is_trait_getter(l: &Lower, s: SymbolId) -> bool {
    has(l.u, s, FIELDaccessor) && !is_setter(l, s) && !has(l.u, s, LAZY)
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
        let (Tree::DefDef { name, paramss, tpt, .. }, Some(s)) = (l.tpd.tree(stat).clone(), l.tpd.sym(stat))
        else {
            continue;
        };
        if !has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY) || is_const_final(l, s) {
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
    let getter_name = l.u.symbol(getter).name.clone();
    let name = trait_setter_name(l, trait_, &getter_name);
    let setter = l.u.define(symbol(&name, Kind::Def, trait_, tasty::Flags::default().with(FIELDaccessor)));
    let getter_info = l.u.info(getter);
    let value = l.u.result_type(getter_info);
    let unit = l.u.class_ref("scala.Unit");
    let binder = l.u.fresh_binder();
    let params = vec![("x$0".to_owned(), value)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: unit, implicit: false, contextual: false });
    l.u.set_info(setter, info);
    l.u.set_deferred(setter, true);
    let param = l.u.define(symbol("x$0", Kind::Param, setter, tasty::Flags::default()));
    l.u.set_info(param, value);
    let param = l.param_def(param, span);
    let params = l.tpd.list(&[param]);
    let paramss = l.tpd.clauses(&[params]);
    let tpt = l.type_tree(unit, span);
    let name = l.term_name(&name);
    l.tpd.node(Tree::DefDef { name, paramss, tpt, rhs: None }, span, None, Some(setter))
}

/// The members class `class` gets from trait `mixin` it mixes in (dotty's `traitInits`,
/// `superCallOpt` and `setters`): a getter without a body for each value it does not override,
/// the call `super[M].$init$()`, a setter with a `()` body for each setter; the call and the
/// bodies at the point of the class's `span`, the members without spans, which copying the
/// template gives them.
pub fn mixed_in(l: &mut Lower, class: SymbolId, mixin: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    let mut out = Vec::new();
    let decls = l.u.decls(mixin).unwrap_or_default();
    for &getter in &decls {
        if is_trait_getter(l, getter) && !is_const_final(l, getter) && implemented_by(l, class, getter) {
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

/// Whether the member of `class` named as `getter` is `getter` (dotty's `isInImplementingClass`).
fn implemented_by(l: &mut Lower, class: SymbolId, getter: SymbolId) -> bool {
    let name = l.u.symbol(getter).name.clone();
    for base in l.u.linearization(class) {
        let found = l.u.decl(base, &name).unwrap_or_default();
        if let Some(&m) = found
            .iter()
            .find(|&&m| matches!(l.u.symbol(m).kind, Kind::Val | Kind::Def) && !has(l.u, m, PRIVATE))
        {
            return m == getter;
        }
    }
    false
}

/// A copy in `class` of accessor `member` with body `rhs` (dotty's `mkForwarderSym`), without a
/// span.
fn forwarder(l: &mut Lower, class: SymbolId, member: SymbolId, rhs: Option<TNodeId>) -> TNodeId {
    let span = Span::NONE;
    let m = l.u.symbol(member);
    let (name, kind, flags) = (m.name.clone(), m.kind, m.flags.with(SYNTHETIC));
    let sym = l.u.define(symbol(&name, kind, class, flags));
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
    let clauses = if kind == Kind::Def { vec![l.tpd.list(&defs)] } else { Vec::new() };
    let paramss = l.tpd.clauses(&clauses);
    let result = l.u.result_type(info);
    let tpt = l.type_tree(result, span);
    let name = l.term_name(&name);
    l.tpd.node(Tree::DefDef { name, paramss, tpt, rhs }, span, None, Some(sym))
}
