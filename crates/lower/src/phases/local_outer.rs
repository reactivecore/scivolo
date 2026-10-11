//! dotty's `ExplicitOuter` for local classes: one defined in a term that is not the method of a
//! superclass argument of a static class takes the instance of its enclosing class if one of
//! its parents may take one (`needsOuterAlways`), or if it calls a local method of a class
//! enclosing it that is not static (`referencesOuter`). An anonymous class keeps only the
//! constructor parameter (dotty's `DropOuterAccessors`).

use super::explicit_outer::enclosing_instance;
use super::local_free::{enclosing_class, is_term, within};
use super::walk::subtrees;
use crate::Lower;
use crate::symbols::{has, is_static_owner};
use tasty::tags::LAZY;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The class whose instance local class `class` of definition `td` takes, if any.
pub fn local_outer(l: &mut Lower, td: TNodeId, class: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(class).owner.filter(|&o| is_term(l, o))?;
    if !needs_outer_if_referenced(l, class) {
        return None;
    }
    let outer = enclosing_class(l, owner);
    let parents: Vec<SymbolId> = l.u.parents(class).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    let always = parents.into_iter().any(|p| needs_outer_if_referenced(l, p));
    (always || references_outer(l, td, class)).then_some(outer)
}

/// Whether class `c` takes the instance of its enclosing class where it uses it (dotty's
/// `needsOuterIfReferenced`), a local value not owning what its right-hand side defines.
pub fn needs_outer_if_referenced(l: &Lower, c: SymbolId) -> bool {
    let Some(mut owner) = l.u.symbol(c).owner else { return false };
    if !is_term(l, owner) {
        return enclosing_instance(l, c).is_some();
    }
    while l.u.symbol(owner).kind == Kind::Val && !has(l.u, owner, LAZY) {
        match l.u.symbol(owner).owner.filter(|&o| is_term(l, o)) {
            Some(o) => owner = o,
            None => break,
        }
    }
    let super_arg = l.u.symbol(owner).kind == Kind::Def && l.u.symbol(owner).name.contains("$superArg$");
    !(l.u.is_trait(c) || super_arg && is_static_owner(l.u, enclosing_class(l, owner)))
}

/// Whether local class definition `td` of `class` calls a local method of a class enclosing it
/// that is not static.
fn references_outer(l: &Lower, td: TNodeId, class: SymbolId) -> bool {
    subtrees(l.tpd, td).into_iter().any(|t| {
        let (Tree::Ident { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else { return false };
        let Some(owner) = l.u.symbol(s).owner.filter(|&o| is_term(l, o)) else { return false };
        let enclosing = enclosing_class(l, owner);
        l.u.symbol(s).kind == Kind::Def
            && !is_static_owner(l.u, enclosing)
            && class != enclosing
            && within(l, class, enclosing)
    })
}

/// Whether local class `class` is anonymous.
pub fn is_anonymous(l: &Lower, class: SymbolId) -> bool {
    l.u.symbol(class).name.starts_with("$anon")
}

/// The prefix of the name of the accessor of the enclosing instance of local class `class`:
/// its enclosing class's name, without an object's `$`, a `_$` for each term it is in, then its
/// name (dotty's `fullNameSeparated`).
pub fn accessor_prefix(l: &Lower, class: SymbolId) -> String {
    let mut filler = String::new();
    let mut owner = l.u.symbol(class).owner.expect("a local class's owner");
    while is_term(l, owner) {
        filler.push_str("_$");
        owner = l.u.symbol(owner).owner.expect("a term's owner");
    }
    let enclosing = l.u.binary_name(owner).replace('.', "$");
    let enclosing = enclosing.strip_suffix('$').filter(|_| l.u.is_module_class(owner)).unwrap_or(&enclosing);
    format!("{enclosing}${filler}{}", l.u.symbol(class).name)
}
