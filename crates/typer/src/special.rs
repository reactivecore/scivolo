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

/// dotty's `<special-ops>.throw(x: Throwable): Nothing`, defined if no one has.
pub fn throw_method(u: &mut Universe) -> SymbolId {
    let ops = u.enter_package("<special-ops>");
    if let Some(t) = entered(u, ops, "throw", Kind::Def) {
        return t;
    }
    let throwable = u.class_ref("java.lang.Throwable");
    let nothing = type_ref(u, u.builtins().nothing);
    let binder = u.fresh_binder();
    let params = vec![("x".into(), throwable)];
    let info =
        u.intern_type(Type::Method { binder, params, result: nothing, implicit: false, contextual: false });
    let throw = special(u, ops, "throw", Kind::Def, Flags::default());
    u.set_info(throw, info);
    throw
}
