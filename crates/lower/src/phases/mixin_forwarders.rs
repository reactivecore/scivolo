// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MixinOps.scala
//! The trait methods a class gets mixin forwarders to (dotty's `MixinOps.needsMixinForwarder`).

use super::function_classes::{erased_classes, erased_linearization};
use super::mixin::has_body;
use crate::Lower;
use crate::symbols::{beyond_jdk_view, has};
use names::{name_kinds, nme};
use rustc_hash::FxHashSet;
use tasty::tags::{FIELDaccessor, INLINE, PRIVATE, STATIC};
use tasty::{Kind, SymbolId, TypeId};

/// The methods of trait `mixin` that `class` gets forwarders to.
pub fn forwarded(l: &mut Lower, class: SymbolId, mixin: SymbolId) -> Vec<SymbolId> {
    let decls = l.u.decls(mixin).unwrap_or_default();
    decls
        .into_iter()
        .filter(|&meth| {
            has_body(l, meth)
                && !has(l.u, meth, INLINE)
                && !beyond_jdk_view(l.u, meth)
                && needs_forwarder(l, class, meth)
                && (l.force_forwarders || needed_unforced(l, class, meth))
        })
        .collect()
}

/// Whether `meth` is a method `class` inherits as it is: the first non-private concrete method
/// or value of its name and erased signature, or matching it, in the linearization of `class`,
/// bridges and inline accessors aside, if `class` inherits it.
fn needs_forwarder(l: &mut Lower, class: SymbolId, meth: SymbolId) -> bool {
    let m = l.u.symbol(meth);
    if m.kind != Kind::Def
        || m.name
            .rsplit(name_kinds::EXPAND_SEPARATOR)
            .next()
            .is_some_and(|n| n.starts_with(name_kinds::INLINE_PREFIX))
        || l.u.is_constructor(meth)
        || has(l.u, meth, PRIVATE)
        || has(l.u, meth, FIELDaccessor)
        || has(l.u, meth, STATIC)
    {
        return false;
    }
    let site = l.u.this_type(class);
    if !inherits(l, class, meth, site, &mut FxHashSet::default()) {
        return false;
    }
    for base in erased_linearization(l, class) {
        for other in matching_decls(l, base, meth, site) {
            if !is_deferred(l, other) {
                return other == meth;
            }
        }
    }
    false
}

/// Whether `class` needs a forwarder to inherited trait method `meth` under
/// `-Xmixin-force-forwarders:false`: another base class than its trait has a concrete method it
/// overrides or any method of a class it overrides, or it is `readResolve()` or `writeReplace()`.
fn needed_unforced(l: &mut Lower, class: SymbolId, meth: SymbolId) -> bool {
    let name = l.u.symbol(meth).name.clone();
    if matches!(name.as_str(), nme::READ_RESOLVE | nme::WRITE_REPLACE) && l.u.erased_method(meth).0.is_empty()
    {
        return true;
    }
    let owner = l.u.symbol(meth).owner;
    let site = l.u.this_type(class);
    for base in erased_linearization(l, class) {
        if Some(base) == owner {
            continue;
        }
        for other in matching_decls(l, base, meth, site) {
            if !is_deferred(l, other) || !l.u.is_trait(base) {
                return true;
            }
        }
    }
    false
}

/// Whether member `s` has no body: in its TASTy for the library's, else if it is a deferred
/// method.
fn is_deferred(l: &mut Lower, s: SymbolId) -> bool {
    match l.u.symbol(s).tree.is_some() {
        true => l.u.body(s).is_none(),
        false => l.u.symbol(s).kind == Kind::Def && !has(l.u, s, FIELDaccessor) && !has_body(l, s),
    }
}

/// Whether `meth` is a member of `class`, not hidden by a matching declaration of `class` or of
/// a base class between them (dotty's `nonPrivateMembersNamed`, `mapInherited`).
fn inherits(
    l: &mut Lower,
    class: SymbolId,
    meth: SymbolId,
    site: TypeId,
    seen: &mut FxHashSet<SymbolId>,
) -> bool {
    let Some(owner) = l.u.symbol(meth).owner else { return false };
    if class == owner {
        return true;
    }
    if !seen.insert(class) || !matching_decls(l, class, meth, site).is_empty() {
        return false;
    }
    let parents: Vec<SymbolId> = l.u.parents(class).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    let parents = erased_classes(l, parents);
    parents
        .into_iter()
        .any(|p| erased_linearization(l, p).contains(&owner) && inherits(l, p, meth, site, seen))
}

/// The non-private methods and values of `base`, bridges aside, with the name and erased
/// signature of `meth`, or matching it as members of `site`.
fn matching_decls(l: &mut Lower, base: SymbolId, meth: SymbolId, site: TypeId) -> Vec<SymbolId> {
    let name = l.u.symbol(meth).name.clone();
    let signature = l.u.erased_method(meth);
    let Lower { u, bridges, .. } = l;
    let mut decls = u.decls_named(base, &name);
    decls.retain(|&other| {
        matches!(u.symbol(other).kind, Kind::Def | Kind::Val)
            && !has(u, other, PRIVATE)
            && !bridges.contains(&other)
            && (u.erased_method(other) == signature || u.info_matches(site, other, meth))
    });
    decls
}
