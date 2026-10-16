//! The names `Mixin` gives accessors: trait setters `T$_setter_$x_=` (dotty's `TraitSetterName`)
//! and the expanded names `p$T$$x` of private trait members (dotty's `expandedName`).

use crate::Lower;
use crate::symbols::has;
use tasty::SymbolId;
use tasty::tags::{FIELDaccessor, LOCAL, PRIVATE};

/// The separator in the names of trait setters.
const TRAIT_SETTER: &str = "$_setter_$";

/// The name of trait `trait_`'s setter of value `x`: `T$_setter_$x_=`, `T` the names of the
/// trait and its owners joined by `$`.
pub fn trait_setter_name(l: &Lower, trait_: SymbolId, x: &str) -> String {
    format!("{}{TRAIT_SETTER}{x}_=", l.u.expanded_prefix(trait_))
}

/// The name of the getter of setter `x_=` or of trait setter `T$_setter_$x_=`.
pub fn setter_getter_name(setter: &str) -> &str {
    let name = setter.rsplit(TRAIT_SETTER).next().unwrap_or(setter);
    name.strip_suffix("_=").unwrap_or(name)
}

/// The name of accessor `s` outside its class: a private trait member's expanded.
pub fn accessor_name(l: &Lower, s: SymbolId) -> String {
    let sym = l.u.symbol(s);
    match sym.owner {
        Some(o) if has(l.u, s, PRIVATE) && l.u.is_trait(o) => {
            format!("{}$${}", l.u.expanded_prefix(o), run_name(l, s))
        }
        _ => run_name(l, s).to_owned(),
    }
}

/// The name of `s` at run time: its `@targetName`, else its name.
pub fn run_name<'a>(l: &'a Lower, s: SymbolId) -> &'a str {
    let sym = l.u.symbol(s);
    sym.target.as_deref().unwrap_or(&sym.name)
}

/// Gives the private accessors of trait `trait_` their expanded names and makes them public.
pub fn expand_private(l: &mut Lower, trait_: SymbolId) {
    for s in l.u.decls(trait_).unwrap_or_default() {
        if has(l.u, s, FIELDaccessor) && has(l.u, s, PRIVATE) {
            ensure_not_private(l, s);
        }
    }
}

/// Gives private member `s` other than a constructor its expanded name `p$C$$x` (`p$O$$$x` in
/// object `O`), `C` the class a lifted method was defined in with a `_$` before `x` for each
/// term it was in, a class constructor's `$init$` spelled `$lessinit$greater`, and makes it
/// neither private nor local (dotty's `ensureNotPrivate`).
pub fn ensure_not_private(l: &mut Lower, s: SymbolId) {
    if !l.u.is_constructor(s) {
        let mut filler = String::new();
        let mut from = l.lifted_from.get(&s).copied();
        while let Some(term) = from.filter(|&t| l.u.symbol(t).kind.is_term()) {
            filler.push_str("_$");
            let term = l.lifted_to.get(&term).copied().unwrap_or(term);
            from = l.u.symbol(term).owner;
        }
        let base = from.or(l.u.symbol(s).owner).expect("a member's owner");
        let module = if l.u.symbol(base).name.ends_with('$') { "$" } else { "" };
        let own = &l.u.symbol(s).name;
        let own = match own.strip_prefix("$init$$") {
            Some(rest) if !filler.is_empty() && !l.u.is_trait(base) => format!("$lessinit$greater${rest}"),
            _ => own.clone(),
        };
        let name = format!("{}{module}$${filler}{own}", l.u.expanded_prefix(base));
        l.u.rename(s, &name);
    }
    let flags = l.u.symbol(s).flags.without(PRIVATE).without(LOCAL);
    l.u.set_flags(s, flags);
}
