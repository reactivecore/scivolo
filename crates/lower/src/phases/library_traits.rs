// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Getters.scala
//   compiler/src/dotty/tools/dotc/transform/Mixin.scala
//! The accessors dotty's `Getters` and `Mixin` give the values of a library trait beyond its
//! TASTy: each concrete value a getter, a `val` a trait setter.

use super::accessor_names::{accessor_name, trait_setter_name};
use super::getters::is_const_final;
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};

/// Gives the concrete values of library trait `trait_` their getters and setters, once.
pub fn prepare(l: &mut Lower, trait_: SymbolId, span: Span) -> Result<()> {
    for value in l.u.decls(trait_).unwrap_or_default() {
        if l.u.symbol(value).kind != Kind::Val || has(l.u, value, FIELDaccessor) {
            continue;
        }
        if l.u.is_module(value) {
            return unsupported(span, "objects in library traits");
        }
        if l.u.body(value).is_none() || is_const_final(l, value) {
            continue;
        }
        let flags = l.u.symbol(value).flags.with(FIELDaccessor);
        l.u.set_flags(value, flags);
        if !has(l.u, value, MUTABLE) && !has(l.u, value, LAZY) {
            let info = l.u.info(value);
            let setter = trait_setter_name(l, trait_, &accessor_name(l, value));
            define_setter(l, trait_, &setter, info);
        }
    }
    Ok(())
}

/// Whether library trait `trait_` has a lazy value with an initializer.
pub fn has_lazy(l: &mut Lower, trait_: SymbolId) -> bool {
    let decls = l.u.decls(trait_).unwrap_or_default();
    decls.into_iter().any(|v| l.u.symbol(v).kind == Kind::Val && has(l.u, v, LAZY) && l.u.body(v).is_some())
}

/// The deferred setter `name(x$0: value): Unit` of class `owner`.
pub fn define_setter(l: &mut Lower, owner: SymbolId, name: &str, value: TypeId) -> SymbolId {
    let setter = l.u.define(symbol(name, Kind::Def, owner, Flags::default().with(FIELDaccessor)));
    let unit = l.u.class_ref("scala.Unit");
    let binder = l.u.fresh_binder();
    let params = vec![("x$0".to_owned(), value)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: unit, implicit: false, contextual: false });
    l.u.set_info(setter, info);
    l.u.set_deferred(setter, true);
    setter
}
