// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Skolemized prefixes of selections whose qualifier is no path (dotty's `QualSkolemType`).

use super::Typer;
use super::stability::is_stable_path;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// `prefix`, or the skolem of it if it is no path and the type of member `m` seen from it
    /// would be approximated (dotty's `maybeSkolemizePrefix` and `asSeenFrom` of a
    /// `QualSkolemType`); one skolem for each such prefix.
    pub fn qualifier_prefix(&mut self, prefix: TypeId, m: SymbolId) -> TypeId {
        let path_like = !matches!(self.u.types().get(prefix), Type::Sym(..) | Type::Applied(..));
        if !self.u.symbol(m).kind.is_term() || path_like || is_stable_path(self.u, prefix) {
            return prefix;
        }
        let info = self.u.info(m);
        if !approximated(self.u, info, 1) {
            return prefix;
        }
        if let Some(&s) = self.qual_skolems.get(&prefix) {
            return s;
        }
        let s = self.u.fresh_skolem(prefix);
        self.qual_skolems.insert(prefix, s);
        s
    }

    /// Whether every skolem in `t` is the skolem of a selection's prefix.
    pub fn only_qual_skolems(&mut self, t: TypeId) -> bool {
        let mut only = true;
        let qual = &self.qual_skolems;
        self.u.map_type(t, &mut |u, part| {
            if matches!(u.types().get(part), Type::Skolem(..)) && !qual.values().any(|&s| s == part) {
                only = false;
            }
            None
        });
        only
    }
}

/// Whether `t` selects an abstract type member of a `this` in a position of variance `variance`
/// or below, which a prefix that is no path approximates (dotty's `approxCount`).
fn approximated(u: &mut Universe, t: TypeId, variance: i8) -> bool {
    match u.types().get(t).clone() {
        Type::Method { params, result, .. } => {
            params.iter().any(|&(_, p)| approximated(u, p, -variance)) || approximated(u, result, variance)
        }
        Type::Poly { result, .. } => approximated(u, result, variance),
        Type::ByName(r) | Type::Annotated(r, _) => approximated(u, r, variance),
        Type::And(a, b) | Type::Or(a, b, _) => approximated(u, a, variance) || approximated(u, b, variance),
        Type::Applied(tycon, args) => {
            let variances = u.constructor_variances(tycon);
            args.iter().enumerate().any(|(i, &a)| {
                let v = variances.get(i).copied().unwrap_or(0);
                approximated(u, a, variance * v)
            })
        }
        Type::Sym(Some(pre), s) => {
            let abstract_member = u.symbol(s).kind == Kind::Type && u.alias_of(s).is_none();
            variance <= 0 && abstract_member && matches!(u.types().get(pre), Type::This(_))
        }
        _ => false,
    }
}
