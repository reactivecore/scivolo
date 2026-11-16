// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala

use super::type_ops::TypeOps;
use tasty::{ANY, Kind, NOTHING, Type, TypeId, Universe};

impl TypeOps<'_> {
    /// `t` at variance `v` with each wildcard replaced by its upper bound where it occurs
    /// covariantly, its lower bound contravariantly and a wildcard argument of both as an
    /// invariant type argument, a type with one `Nothing` at a negative variance (dotty's
    /// `AvoidWildcardsMap`).
    ///
    /// Refuses a wildcard in a type other than an applied class type, an intersection or a union.
    pub fn avoid_wildcards(&mut self, t: TypeId, v: i8) -> Result<TypeId, &'static str> {
        if !self.has_wildcard(t) {
            return Ok(t);
        }
        Ok(match *self.u.ty(t) {
            Type::Wildcard(bounds) => {
                let (lo, hi) = bounds.and_then(|b| self.u.bounds(b)).unwrap_or((NOTHING, ANY));
                match v {
                    -1 => self.avoid_wildcards(lo, -1)?,
                    _ => self.avoid_wildcards(hi, 1)?,
                }
            }
            Type::Applied(tycon, args) => {
                let Some(c) = self.u.sym_of(tycon).filter(|&c| self.u.symbol(c).kind == Kind::Class) else {
                    return Err("signature polymorphic call expected to be of a type with a wildcard");
                };
                let variances = self.u.variances(c);
                let mut avoided = Vec::new();
                for (i, a) in self.u.types_of(args).to_vec().into_iter().enumerate() {
                    avoided.push(match variances.get(i).copied().unwrap_or(0) {
                        0 if self.has_wildcard(a) && v == -1 => return Ok(NOTHING),
                        0 if self.has_wildcard(a) => {
                            let (lo, hi) = (self.avoid_wildcards(a, -1)?, self.avoid_wildcards(a, 1)?);
                            let lo = self.u.bounds(lo).map_or(lo, |(l, _)| l);
                            let hi = self.u.bounds(hi).map_or(hi, |(_, h)| h);
                            self.u.intern_type(Type::Bounds(lo, hi))
                        }
                        0 => a,
                        av => self.avoid_wildcards(a, av * v)?,
                    });
                }
                self.u.intern_applied(tycon, &avoided)
            }
            Type::And(a, b) => {
                let (a, b) = (self.avoid_wildcards(a, v)?, self.avoid_wildcards(b, v)?);
                self.u.intern_type(Type::And(a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (self.avoid_wildcards(a, v)?, self.avoid_wildcards(b, v)?);
                self.u.intern_type(Type::Or(a, b, soft))
            }
            _ => return Err("signature polymorphic call expected to be of a type with a wildcard"),
        })
    }

    /// Whether `t` mentions a wildcard.
    fn has_wildcard(&mut self, t: TypeId) -> bool {
        has_part(self.u, t, |u, part| matches!(u.ty(part), Type::Wildcard(_)))
    }
}

/// Whether `t` has a part `is` holds of.
pub fn has_part(u: &mut Universe, t: TypeId, is: impl Fn(&Universe, TypeId) -> bool) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= is(u, part);
        None
    });
    found
}
