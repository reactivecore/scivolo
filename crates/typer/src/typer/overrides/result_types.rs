// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/transform/OverridingPairs.scala
//! dotty's "has incompatible type" check of `checkOverride`: an overriding member's type must
//! conform to the overridden one's (`isOverridingPair`).

use super::super::Typer;
use super::result_conformance::{conforms_overridden, result_conforms};
use super::{OverrideChecks, overridable};
use names::{name_kinds, nme};
use syntax::ast::flags as f;
use tasty::tags::TRAIT;
use tasty::{Kind, SymbolId, Type, TypeId};

impl OverrideChecks {
    /// Whether a member of `own`, the class's declarations, or an inherited one overrides another
    /// with an incompatible type.
    pub fn incompatible_type(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> bool {
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
    /// seen from the class's `this` or else from the class; those of a higher-kinded member as
    /// type lambdas.
    fn bounds_within(&self, t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> bool {
        let this = t.u.intern_type(Type::This(self.site));
        [this, self.site].into_iter().any(|prefix| {
            let bounds = |t: &mut Typer<'_>, s: SymbolId| {
                let info = t.u.own_member_info(prefix, s);
                let node = *t.u.ty(info);
                match node {
                    Type::Alias(a) => Some((a, a)),
                    Type::Bounds(lo, hi) => Some((lo, hi)),
                    Type::Lambda { binder, params, result } => Some(match t.u.bounds(result) {
                        Some((lo, hi)) => {
                            let mut lambda = |x| t.u.intern_type(Type::Lambda { binder, params, result: x });
                            (lambda(lo), lambda(hi))
                        }
                        None => (info, info),
                    }),
                    _ => None,
                }
            };
            let (Some((lo1, hi1)), Some((lo2, hi2))) = (bounds(t, d), bounds(t, m)) else { return true };
            let site = self.site;
            let conforms = |t: &mut Typer<'_>, a, b| {
                let (a, b) = (params_at_upper_bounds(t, a), params_at_upper_bounds(t, b));
                conforms_overridden(t, site, a, b) || derives_nominally(t, a, b)
            };
            conforms(t, lo2, lo1) && conforms(t, hi1, hi2)
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
    /// type; members of two Java classes, not interfaces, are not compared.
    pub fn incompatible_pair(&self, t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> bool {
        let java_class = |t: &mut Typer<'_>, s: SymbolId| {
            let owner = t.u.symbol(s).owner.is_some_and(|o| !t.u.symbol(o).flags.has(TRAIT));
            owner && t.is_java_member(s)
        };
        let java = java_class(t, d) && java_class(t, m);
        !java
            && t.u.symbol(d).run_name() == t.u.symbol(m).run_name()
            && t.u.info_matches(self.site, d, m)
            && !result_conforms(t, self.site, d, m)
    }
}

/// Whether `a` and `b` refer to classes without type arguments and `a`'s derives from `b`'s, as
/// `class Node extends super.NodeImpl` does from `NodeImpl`.
fn derives_nominally(t: &mut Typer<'_>, a: TypeId, b: TypeId) -> bool {
    let class = |t: &Typer<'_>, x: TypeId| match *t.u.ty(x) {
        Type::Sym(_, s) if t.u.symbol(s).kind == Kind::Class => Some(s),
        _ => None,
    };
    let (Some(ca), Some(cb)) = (class(t, a), class(t, b)) else { return false };
    t.u.derives_from(ca, cb)
}

/// Type lambda `ty` with its bounded parameters replaced in its body by their upper bounds, which
/// a comparison of lambda bodies does not know.
fn params_at_upper_bounds(t: &mut Typer<'_>, ty: TypeId) -> TypeId {
    let Type::Lambda { binder, params, result } = *t.u.ty(ty) else { return ty };
    let list = t.u.params_of(params).to_vec();
    let names: Vec<_> = list.iter().map(|&(n, _)| n).collect();
    let uppers: Vec<_> = list
        .iter()
        .enumerate()
        .map(|(i, &(_, info))| match t.u.bounds(info) {
            Some((_, hi)) if hi != tasty::ANY => hi,
            _ => t.u.intern_type(Type::Param(binder, i as u32)),
        })
        .collect();
    let result = t.u.subst(result, binder, &names, &uppers);
    t.u.intern_type(Type::Lambda { binder, params, result })
}

/// Whether `d` is a value or method other than a constructor or default getter.
pub fn is_checked_term(t: &Typer<'_>, d: SymbolId) -> bool {
    let (kind, name) = (t.u.symbol(d).kind, &t.u.symbol(d).name);
    let exempt = name == nme::CONSTRUCTOR || name.contains(name_kinds::DEFAULT_GETTER);
    matches!(kind, Kind::Def | Kind::Val) && !exempt
}
