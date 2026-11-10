// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! The type variables of a match type case avoided in its body's type (dotty's `TypeOps.avoid`
//! of `bindTypeSymbols`): by their upper bound where they occur covariantly, their lower bound
//! contravariantly, a wildcard of both as an invariant argument.

use super::type_ops::TypeOps;
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId};

impl TypeOps<'_> {
    /// Approximates the body type `t` of a match type case such as `case List[x] => Option[x]` by
    /// a type that no longer mentions the bound type variables `captures`: each variable by its
    /// upper bound where a supertype will do, its lower bound where a subtype will, and a
    /// wildcard of both as an invariant type argument.
    pub fn avoid_captures(&mut self, t: TypeId, captures: &[SymbolId], variance: i8) -> TypeId {
        if !self.refers_to(t, captures) {
            return t;
        }
        match self.u.ty(t).clone() {
            Type::Sym(_, s) if captures.contains(&s) => {
                let info = self.u.info(s);
                let (lo, hi) = self.u.bounds(info).unwrap_or((NOTHING, ANY));
                match variance {
                    1 => hi,
                    -1 => lo,
                    _ => self.u.intern_type(Type::Bounds(lo, hi)),
                }
            }
            Type::Applied(tycon, args) => match self.u.sym_of(tycon) {
                Some(c) if self.u.symbol(c).kind == Kind::Class => {
                    let variances = self.u.variances(c);
                    let args: Vec<_> = (self.u.types_of(args).to_vec().into_iter().enumerate())
                        .map(|(i, a)| match variances.get(i).copied().unwrap_or(0) {
                            0 => self.avoid_invariant(a, captures),
                            v => self.avoid_captures(a, captures, v * variance),
                        })
                        .collect();
                    self.u.intern_applied(tycon, &args)
                }
                _ => match self.u.dealias(t) {
                    d if d != t => self.avoid_captures(d, captures, variance),
                    _ => self.extreme(variance),
                },
            },
            Type::And(a, b) => {
                let (a, b) =
                    (self.avoid_captures(a, captures, variance), self.avoid_captures(b, captures, variance));
                self.u.intern_type(Type::And(a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) =
                    (self.avoid_captures(a, captures, variance), self.avoid_captures(b, captures, variance));
                self.u.intern_type(Type::Or(a, b, soft))
            }
            _ => self.extreme(variance),
        }
    }

    /// Invariant argument `a` without references to `captures`: itself, or a wildcard of its
    /// approximations from below and above.
    fn avoid_invariant(&mut self, a: TypeId, captures: &[SymbolId]) -> TypeId {
        if !self.refers_to(a, captures) {
            return a;
        }
        let (lo, hi) = (self.avoid_captures(a, captures, -1), self.avoid_captures(a, captures, 1));
        let bound = |t: &mut Self, x: TypeId, at_lo: bool| {
            t.u.bounds(x).map_or(x, |(l, h)| if at_lo { l } else { h })
        };
        let (lo, hi) = (bound(self, lo, true), bound(self, hi, false));
        self.u.intern_type(Type::Bounds(lo, hi))
    }

    /// The type every type is a subtype of at `variance` 1, the one below all at -1.
    fn extreme(&mut self, variance: i8) -> TypeId {
        match variance {
            -1 => NOTHING,
            _ => ANY,
        }
    }
}
