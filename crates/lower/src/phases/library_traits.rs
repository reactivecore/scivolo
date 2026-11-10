// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Getters.scala
//   compiler/src/dotty/tools/dotc/transform/Mixin.scala
//! The accessors dotty's `Getters` and `Mixin` give the values of a library trait beyond its
//! TASTy: each concrete value a getter, a `var` a setter, a `val` a trait setter.

use super::accessor_names::{accessor_name, trait_setter_name};
use super::getters::is_const_final;
use crate::Lower;
use crate::definitions::symbol;
use crate::symbols::has;
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE, PRIVATE};
use tasty::{Flags, Kind, SymbolId, TypeId};

/// Gives the concrete values of library trait `trait_` their getters and setters, once.
pub fn prepare(l: &mut Lower, trait_: SymbolId) {
    for value in l.u.decls(trait_).unwrap_or_default() {
        if l.u.symbol(value).kind != Kind::Val
            || has(l.u, value, FIELDaccessor)
            || l.u.is_module(value)
            || l.u.body(value).is_none()
        {
            continue;
        }
        if is_const_final(l, value) {
            if has(l.u, value, PRIVATE) {
                super::accessor_names::ensure_not_private(l, value);
            }
            continue;
        }
        let flags = l.u.symbol(value).flags.with(FIELDaccessor);
        l.u.set_flags(value, flags);
        let info = l.u.info(value);
        if has(l.u, value, MUTABLE) {
            let setter = format!("{}_=", l.u.symbol(value).name);
            if l.u.decl(trait_, &setter).is_ok_and(|d| d.is_empty()) {
                define_setter(l, trait_, &setter, info);
            }
        } else if !has(l.u, value, LAZY) {
            let setter = trait_setter_name(l, trait_, &accessor_name(l, value));
            define_setter(l, trait_, &setter, info);
        }
    }
}

/// Whether library trait `trait_` has an object or a lazy value with an initializer.
pub fn has_lazy(l: &mut Lower, trait_: SymbolId) -> bool {
    let decls = l.u.decls(trait_).unwrap_or_default();
    decls.into_iter().any(|v| {
        l.u.symbol(v).kind == Kind::Val && (l.u.is_module(v) || has(l.u, v, LAZY) && l.u.body(v).is_some())
    })
}

/// The deferred setter `name(x$0: value): Unit` of class `owner`.
pub fn define_setter(l: &mut Lower, owner: SymbolId, name: &str, value: TypeId) -> SymbolId {
    let setter = l.u.define(symbol(name, Kind::Def, owner, Flags::default().with(FIELDaccessor)));
    let unit = l.u.class_ref("scala.Unit");
    let binder = l.u.fresh_binder();
    let params = [(l.u.intern_name("x$0"), value)];
    let info = l.u.intern_method(binder, &params, unit, false, false);
    l.u.set_info(setter, info);
    l.u.set_deferred(setter, true);
    setter
}
