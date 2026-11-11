// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! Symbols the typer enters itself where the library has none.

use crate::defn::type_ref;
use tasty::{Flags, Kind, Symbol, SymbolId, Type, Universe};

/// The declaration `name` of `kind` already entered in `owner`, loading nothing.
pub fn entered(u: &Universe, owner: SymbolId, name: &str, kind: Kind) -> Option<SymbolId> {
    u.symbol(owner).decls.iter().copied().find(|&d| u.symbol(d).name == name && u.symbol(d).kind == kind)
}

/// The symbol `name` of kind `kind` in `owner`, defined with `flags` if no one has.
pub fn special(u: &mut Universe, owner: SymbolId, name: &str, kind: Kind, flags: Flags) -> SymbolId {
    if let Some(s) = entered(u, owner, name, kind) {
        return s;
    }
    u.define(Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags,
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    })
}

/// dotty's `<special-ops>.spread[T](x$0: Any): T`, defined if no one has.
pub fn spread_method(u: &mut Universe) -> SymbolId {
    let ops = u.enter_package("<special-ops>");
    if let Some(s) = entered(u, ops, "spread", Kind::Def) {
        return s;
    }
    let (poly, method) = (u.fresh_binder(), u.fresh_binder());
    let param = u.intern_type(Type::Param(poly, 0));
    let params = [(u.intern_name("x$0"), tasty::ANY)];
    let result = u.intern_method(method, &params, param, false, false);
    let bounds = u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY));
    let params = [(u.intern_name("T"), bounds)];
    let info = u.intern_poly(poly, &params, result);
    let spread = special(u, ops, "spread", Kind::Def, Flags::default());
    u.set_info(spread, info);
    spread
}

/// dotty's `<special-ops>.throw(x: Throwable): Nothing`, defined if no one has.
pub fn throw_method(u: &mut Universe) -> SymbolId {
    let ops = u.enter_package("<special-ops>");
    if let Some(t) = entered(u, ops, "throw", Kind::Def) {
        return t;
    }
    let throwable = u.class_ref("java.lang.Throwable");
    let nothing = type_ref(u, u.defn().nothing);
    let binder = u.fresh_binder();
    let params = [(u.intern_name("x"), throwable)];
    let info = u.intern_method(binder, &params, nothing, false, false);
    let throw = special(u, ops, "throw", Kind::Def, Flags::default());
    u.set_info(throw, info);
    throw
}
