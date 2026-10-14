//! dotty's `ExplicitOuter` for local classes: one defined in a term that is not the method of a
//! superclass argument of a static class takes the instance of its enclosing class if one of
//! its parents may take one (`needsOuterAlways`), or if it refers to the instance of a class
//! enclosing it (`referencesOuter`).

use super::local_free::{enclosing_class, is_term};
use super::outer_traits::{is_pure_interface, outer_class};
use super::references_outer::references_outer;
use crate::Lower;
use crate::symbols::{has, is_static_owner};
use tasty::tags::LAZY;
use tasty::{Kind, SymbolId};
use typer::tpd::TNodeId;

/// The class whose instance local class `class` of definition `td` takes, if any.
pub fn local_outer(l: &mut Lower, td: TNodeId, class: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(class).owner.filter(|&o| is_term(l, o))?;
    if !needs_outer_if_referenced(l, class) {
        return None;
    }
    let outer = enclosing_class(l, owner);
    let parents: Vec<SymbolId> = l.u.parents(class).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    let always = parents.into_iter().any(|p| needs_outer_if_referenced(l, p));
    (always || references_outer(l, class, td)).then_some(outer)
}

/// Whether class `c` takes the instance of its enclosing class where it uses it (dotty's
/// `needsOuterIfReferenced` with `outerOwner`): unless a pure interface, or its owner, past values that
/// are not lazy and from a superclass argument's method to its class, is a static owner.
pub fn needs_outer_if_referenced(l: &Lower, c: SymbolId) -> bool {
    let Some(mut owner) = l.u.symbol(c).owner else { return false };
    if !is_term(l, owner) {
        return outer_class(l, c).is_some();
    }
    while l.u.symbol(owner).kind == Kind::Val && !has(l.u, owner, LAZY) {
        let Some(o) = l.u.symbol(owner).owner else { break };
        owner = o;
    }
    if l.u.symbol(owner).kind == Kind::Def && l.u.symbol(owner).name.contains("$superArg$") {
        owner = enclosing_class(l, owner);
    }
    !(l.u.is_trait(c) && is_pure_interface(l.u, c) || is_static_owner(l.u, owner))
}

/// Whether local class `class` is anonymous.
pub fn is_anonymous(l: &Lower, class: SymbolId) -> bool {
    l.u.symbol(class).name.starts_with("$anon")
}

/// The prefix of the name of the accessor of the enclosing instance of class `class` defined in a
/// term or in a class defined in one: its enclosing class's, without an object's `$`, a `_$` for
/// each term it is in, then its name (dotty's `fullNameSeparated`).
pub fn separated_name(l: &Lower, class: SymbolId) -> String {
    let mut filler = String::new();
    let mut owner = l.u.symbol(class).owner.expect("a class's owner");
    while is_term(l, owner) {
        filler.push_str("_$");
        owner = l.u.symbol(owner).owner.expect("a term's owner");
    }
    let enclosing = match super::explicit_outer::in_term(l, owner) {
        true => separated_name(l, owner),
        false => l.u.binary_name(owner).replace('.', "$"),
    };
    let enclosing = enclosing.strip_suffix('$').filter(|_| l.u.is_module_class(owner)).unwrap_or(&enclosing);
    format!("{enclosing}${filler}{}", l.u.symbol(class).name)
}
