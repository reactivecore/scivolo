//! What the definitions of a file and the types its capture sets annotate are to its
//! capture-set scopes.

use crate::tpd::{TNodeId, Tpd, Tree as T};
use names::nme;
use tasty::{Kind, SymbolId, TypeId, Universe};

/// The most aliases a capture-set annotated type is dealiased through to a class.
const MAX_ALIASES: usize = 16;

/// Determines whether `t` is a class type or an alias of one, applied or not.
pub fn is_class_type(u: &mut Universe, mut t: TypeId) -> bool {
    for _ in 0..MAX_ALIASES {
        let tycon = u.applied(t).map_or(t, |(tycon, _)| tycon);
        let Some(s) = u.referenced(tycon) else { return false };
        match u.symbol(s).kind {
            Kind::Class => return true,
            Kind::Type => match u.alias_of(s) {
                Some(a) => t = a,
                None => return false,
            },
            _ => return false,
        }
    }
    false
}

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
