// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/transform/OverridingPairs.scala
//! dotty's "has incompatible type" check of `checkOverride`: an overriding member's type must
//! conform to the overridden one's (`isOverridingPair`).

use super::super::Typer;
use super::result_conformance::result_conforms;
use super::{OverrideChecks, overridable};
use crate::symbols::Completion;
use names::{name_kinds, nme};
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

impl OverrideChecks {
    /// Whether a member of `own`, the class's declarations, or an inherited one overrides another
    /// with an incompatible type, the type members of the unit's base classes completed first.
    pub fn incompatible_type(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> bool {
        for &b in &self.bases {
            let decls = t.u.symbol(b).decls.clone();
            for d in decls {
                let pending = matches!(t.syms.completions.get(&d), Some(Completion::Pending { .. }));
                if pending && t.u.symbol(d).kind == Kind::Type {
                    t.complete(d);
                }
            }
        }
        self.incompatible_term_member(t, own)
            || self.incompatible_type_member(t, own)
            || self.incompatible_inherited(t)
    }

    /// Whether a type member of `own`, the class's declarations, overrides a type member of a
    /// base class with bounds that do not lie within the overridden bounds, as
    /// `type T = String` overriding `type T <: Int`.
    fn incompatible_type_member(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> bool {
        for &d in own {
            if t.u.symbol(d).kind != Kind::Type || t.u.symbol(d).is_private() {
                continue;
            }
            let name = t.u.symbol(d).name.clone();
            for &b in &self.bases {
                let others = t.u.symbol(b).decls.to_vec();
                for m in others {
                    let sm = t.u.symbol(m);
                    if sm.name != name || sm.kind != Kind::Type || sm.is_private() {
                        continue;
                    }
                    t.complete(m);
                    if !self.bounds_within(t, d, m) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Whether the bounds of type member `d`, an alias `(a, a)`, lie within those of `m`, both
    /// seen from the class's `this` or else from the class; higher-kinded members and bounds
    /// with refinements are not compared.
    fn bounds_within(&self, t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> bool {
        let this = t.u.intern_type(Type::This(self.site));
        [this, self.site].into_iter().any(|prefix| {
            let bounds = |t: &mut Typer<'_>, s: SymbolId| {
                let info = t.u.own_member_info(prefix, s);
                match *t.u.ty(info) {
                    Type::Alias(a) => Some((a, a)),
                    Type::Bounds(lo, hi) => Some((lo, hi)),
                    _ => None,
                }
            };
            let (Some((lo1, hi1)), Some((lo2, hi2))) = (bounds(t, d), bounds(t, m)) else { return true };
            if [lo1, hi1, lo2, hi2].into_iter().any(|x| is_uncompared(t, x)) {
                return true;
            }
            t.conforms(lo2, lo1) && t.conforms(hi1, hi2)
        })
    }

    /// Whether a term member of `own` overrides a member of a base class it matches with a result
    /// type that does not conform to the overridden one, as `def y: String` overriding
    /// `def y: Option[Int]`.
    fn incompatible_term_member(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> bool {
        let own = own.iter().copied().filter(|&d| t.syms.flags.get(&d).is_some_and(|fl| !fl.is(f::PRIVATE)));
        own.collect::<Vec<_>>().into_iter().any(|d| self.incompatible_with(t, d, &self.bases))
    }

    /// Whether term member `d` overrides a member of one of classes `bases` it matches with an
    /// incompatible result type.
    fn incompatible_with(&self, t: &mut Typer<'_>, d: SymbolId, bases: &[SymbolId]) -> bool {
        let name = t.u.symbol(d).name.clone();
        if !is_checked_term(t, d) {
            return false;
        }
        for &b in bases {
            for m in overridable(t.u, b, &name) {
                t.complete(m);
                if self.incompatible_pair(t, d, m) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether member `d` overrides member `m`, which it may match, with an incompatible result
    /// type; members of two Java classes are not compared.
    pub fn incompatible_pair(&self, t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> bool {
        let java = t.is_java_member(d) && t.is_java_member(m);
        !java
            && t.u.symbol(d).run_name() == t.u.symbol(m).run_name()
            && t.u.info_matches(self.site, d, m)
            && !result_conforms(t, self.site, d, m)
    }
}

/// Whether bound `ty` is a type lambda or has a refinement, which the type member check skips.
fn is_uncompared(t: &mut Typer<'_>, ty: TypeId) -> bool {
    if matches!(*t.u.ty(ty), Type::Lambda { .. }) {
        return true;
    }
    let mut refined = false;
    t.u.map_type(ty, &mut |u, x| {
        refined |= matches!(*u.ty(x), Type::Refined(..) | Type::Rec(..));
        None
    });
    refined
}

/// Whether `d` is a value or method other than a constructor or default getter.
pub fn is_checked_term(t: &Typer<'_>, d: SymbolId) -> bool {
    let (kind, name) = (t.u.symbol(d).kind, &t.u.symbol(d).name);
    let exempt = name == nme::CONSTRUCTOR || name.contains(name_kinds::DEFAULT_GETTER);
    matches!(kind, Kind::Def | Kind::Val) && !exempt
}
