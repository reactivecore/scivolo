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
use tasty::{Erased, Kind, SymbolId, TypeId, Universe};

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
                && (l.force_forwarders || needed_unforced(l, class, mixin, meth))
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
    let wanted = Wanted::new(l, meth, class);
    if !inherits(l, class, &wanted, &mut FxHashSet::default()) {
        return false;
    }
    for base in erased_linearization(l, class) {
        for other in matching_decls(l, base, &wanted) {
            if !is_deferred(l, other) {
                return other == meth;
            }
        }
    }
    false
}

/// Whether `class` needs a forwarder to method `meth` of trait `mixin` under
/// `-Xmixin-force-forwarders:false`: it overrides a method of a class, or a concrete method of a
/// trait `mixin` does not extend, or it is `readResolve()` or `writeReplace()`.
fn needed_unforced(l: &mut Lower, class: SymbolId, mixin: SymbolId, meth: SymbolId) -> bool {
    let name = l.u.symbol(meth).name.clone();
    if matches!(name.as_str(), nme::READ_RESOLVE | nme::WRITE_REPLACE) && l.u.erased_method(meth).0.is_empty()
    {
        return true;
    }
    let wanted = Wanted::new(l, meth, class);
    let mixin_bases = erased_linearization(l, mixin);
    for base in erased_linearization(l, class) {
        if base == mixin {
            continue;
        }
        let is_trait = l.u.is_trait(base);
        for other in matching_decls(l, base, &wanted) {
            if !is_trait || !is_deferred(l, other) && !mixin_bases.contains(&base) {
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
fn inherits(l: &mut Lower, class: SymbolId, w: &Wanted, seen: &mut FxHashSet<SymbolId>) -> bool {
    let Some(owner) = l.u.symbol(w.meth).owner else { return false };
    if class == owner {
        return true;
    }
    if !seen.insert(class) || !matching_decls(l, class, w).is_empty() {
        return false;
    }
    let parents: Vec<SymbolId> = l.u.parents(class).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    let parents = erased_classes(l, parents);
    parents
        .into_iter()
        .any(|p| erased_linearization(l, p).contains(&owner) && inherits(l, p, w, seen))
}

/// A method looked up in the bases of a class: its name and erased signature, and the class's
/// this-type.
struct Wanted {
    meth: SymbolId,
    name: String,
    signature: (Vec<Erased>, Erased),
    site: TypeId,
}

impl Wanted {
    fn new(l: &mut Lower, meth: SymbolId, class: SymbolId) -> Wanted {
        let site = l.u.this_type(class);
        let signature = l.u.erased_method(meth);
        Wanted { meth, name: l.u.symbol(meth).name.clone(), signature, site }
    }
}

/// The non-private methods and values of `base`, bridges aside, with the name and erased
/// signature of `w.meth`, or matching it as members of `w.site`.
fn matching_decls(l: &mut Lower, base: SymbolId, w: &Wanted) -> Vec<SymbolId> {
    let Lower { u, bridges, .. } = l;
    let mut decls = u.decls_named(base, &w.name);
    decls.retain(|&other| {
        matches!(u.symbol(other).kind, Kind::Def | Kind::Val)
            && !has(u, other, PRIVATE)
            && !bridges.contains(&other)
            && (same_signature(u, other, &w.signature) || u.info_matches(w.site, other, w.meth))
    });
    decls
}

/// Whether method `s` erases to `signature`, compared in the memo of erasures where it is there.
fn same_signature(u: &mut Universe, s: SymbolId, signature: &(Vec<Erased>, Erased)) -> bool {
    match u.erased_memo(s, false) {
        Some(e) => e == signature,
        None => u.erased_method(s) == *signature,
    }
}
