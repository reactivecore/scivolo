// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! Transparent classes and traits, dropped from inferred intersections (dotty's
//! `isTransparent`, `dropTransparentTraits`).

use super::Typer;
use tasty::{Kind, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// `t` without transparent traits among its intersection parts, one at a time while the
    /// result conforms to `bound`; `t` if each dropped trait contains the result (dotty's
    /// `dropTransparentTraits`).
    pub fn without_transparent_traits(&mut self, t: TypeId, bound: TypeId) -> TypeId {
        let mark = self.constraint.mark();
        let (mut kept, mut dropped) = (Vec::new(), Vec::new());
        let mut current = t;
        for _ in 0..32 {
            let narrower = self.drop_one_transparent(current, &kept, &mut dropped);
            if narrower == current {
                break;
            }
            if self.conforms(narrower, bound) {
                current = narrower;
            } else {
                kept.extend(dropped.pop());
            }
        }
        if current == t || dropped.iter().all(|&d| self.u.is_subtype(d, current)) {
            self.constraint.rollback(mark);
            return t;
        }
        current
    }

    /// `t` with its first transparent trait not in `kept` replaced by `Any`, pushed to `dropped`.
    fn drop_one_transparent(&mut self, t: TypeId, kept: &[TypeId], dropped: &mut Vec<TypeId>) -> TypeId {
        if self.is_transparent(t, true) && !kept.contains(&t) {
            dropped.push(t);
            return tasty::ANY;
        }
        let Type::And(a, b) = *self.u.types().get(t) else { return t };
        let a2 = self.drop_one_transparent(a, kept, dropped);
        if a2 != a {
            return self.u.glb(a2, b);
        }
        let b2 = self.drop_one_transparent(b, kept, dropped);
        if b2 != b { self.u.glb(a, b2) } else { t }
    }

    /// Whether `t` is a transparent class or trait, or an intersection of them (dotty's
    /// `isTransparent`); `Any`, `Matchable` and `Object` count unless `trait_only`.
    pub fn is_transparent(&mut self, t: TypeId, trait_only: bool) -> bool {
        let tycon = match *self.u.types().get(t) {
            Type::And(a, b) => {
                return self.is_transparent(a, trait_only) && self.is_transparent(b, trait_only);
            }
            Type::Applied(c, _) => c,
            _ => t,
        };
        let c = match *self.u.types().get(tycon) {
            Type::Sym(_, c) => c,
            Type::TypeRef(..) => match self.u.class_symbol(tycon) {
                Some(c) => c,
                None => return false,
            },
            _ => return false,
        };
        let b = self.u.builtins();
        if !trait_only && (c == b.any || c == b.matchable || c == b.object) {
            return true;
        }
        let s = self.u.symbol(c);
        let assumed = matches!(self.u.full_name(c).as_str(), "java.lang.Comparable" | "java.io.Serializable");
        s.kind == Kind::Class
            && (!trait_only || s.flags.has(tasty::tags::TRAIT))
            && (s.flags.has(tasty::tags::TRANSPARENT) || assumed)
    }
}
