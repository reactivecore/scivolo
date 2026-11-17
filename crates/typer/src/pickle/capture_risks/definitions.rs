//! What the definitions of a file are to its capture-set scopes.

use crate::tpd::{TNodeId, Tpd, Tree as T};
use names::nme;
use tasty::{Kind, SymbolId, Universe};

/// What a definition is to the scopes.
pub enum Def {
    /// A scope: a class, or a value, method or parameter other than a closure's method or its
    /// parameter.
    Scope { class: bool, inferred: bool },
    /// A closure's method or its parameter, in its enclosing scope.
    Nested { inferred: bool },
}

/// Determines the symbol of definition `n` and what it is to the scopes, `None` if `n` is no
/// definition.
pub fn definition(u: &Universe, tpd: &Tpd, n: TNodeId) -> Option<(SymbolId, Def)> {
    let tpt = match *tpd.tree(n) {
        T::ValDef { tpt, .. } | T::DefDef { tpt, .. } => tpt,
        T::TypeDef { .. } => {
            let sym = tpd.sym(n)?;
            return (u.symbol(sym).kind == Kind::Class)
                .then_some((sym, Def::Scope { class: true, inferred: false }));
        }
        _ => return None,
    };
    let sym = tpd.sym(n)?;
    let inferred = matches!(tpd.tree(tpt), T::TypeTree { inferred: true });
    let is_closure = |s: SymbolId| u.symbol(s).name == nme::ANON_FUN;
    let closure_param = u.symbol(sym).kind == Kind::Param && u.symbol(sym).owner.is_some_and(is_closure);
    match closure_param || is_closure(sym) {
        true => Some((sym, Def::Nested { inferred })),
        false => Some((sym, Def::Scope { class: false, inferred })),
    }
}
