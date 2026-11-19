// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! dotty's "has weaker access privileges" check of `checkOverride`.

use super::super::Typer;
use super::result_types::is_checked_term;
use super::{OverrideChecks, overridable};
use crate::sealed::is_contained_in;
use tasty::SymbolId;
use tasty::tags::{PRIVATE, PROTECTED, PROTECTEDqualified, TRAIT};

impl OverrideChecks {
    /// Whether a member of `own`, the class's declarations, or an inherited member overrides
    /// another with weaker access, as protected `f` of `class P { protected def f = 1 }` over `f`
    /// of `trait A { def f: Int }` in `class B extends P with A`.
    pub fn weaker_access(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> bool {
        for &d in own {
            if t.u.symbol(d).is_private() || !is_checked_term(t, d) {
                continue;
            }
            let name = t.u.symbol(d).name.clone();
            for &b in &self.bases {
                for m in overridable(t.u, b, &name) {
                    t.complete(m);
                    if t.same_member(self.site, d, m) && !access_ok(t, d, m) {
                        return true;
                    }
                }
            }
        }
        let pairs = self.inherited_pairs(t);
        pairs.into_iter().any(|(d, m)| !both_java_classes(t, d, m) && !access_ok(t, d, m))
    }
}

/// Whether `member` may override `other` as far as access goes: it is public, or protected if
/// `other` is, with an access boundary containing `other`'s, or `other` is a protected Java
/// member (dotty's `isOverrideAccessOK`).
fn access_ok(t: &mut Typer<'_>, member: SymbolId, other: SymbolId) -> bool {
    let Some(base) = t.u.symbol(member).owner else { return true };
    let Some(mb) = boundary(t, member, base) else { return true };
    let protected = |t: &Typer<'_>, s: SymbolId| {
        let flags = t.u.symbol(s).flags;
        flags.has(PROTECTED) || flags.has(PROTECTEDqualified)
    };
    if protected(t, other) && !protected(t, member) {
        return false;
    }
    let java_protected = t.u.symbol(other).flags.has(PROTECTED) && t.is_java_member(other);
    let contained = boundary(t, other, base).is_some_and(|ob| is_contained_in(t.u, ob, mb));
    contained || java_protected
}

/// The access boundary of `s` seen from class `base`, `None` if public: `base` for a protected
/// member (dotty's `accessBoundary(base)`).
fn boundary(t: &mut Typer<'_>, s: SymbolId, base: SymbolId) -> Option<SymbolId> {
    let found = t.access_boundary(s)?;
    let (flags, owner) = (t.u.symbol(s).flags, t.u.symbol(s).owner);
    let plain_protected = flags.has(PROTECTED) && !flags.has(PRIVATE) && !flags.has(PROTECTEDqualified);
    Some(if plain_protected && Some(found) == owner { base } else { found })
}

/// Whether `d` and `m` are both members of Java classes, which javac checks.
fn both_java_classes(t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> bool {
    [d, m].into_iter().all(|s| {
        let class = t.u.symbol(s).owner.is_some_and(|o| !t.u.symbol(o).flags.has(TRAIT));
        class && t.is_java_member(s)
    })
}
