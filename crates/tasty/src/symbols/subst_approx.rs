// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Substituters.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala

use super::{SymbolId, Universe};
use crate::approx::{self, Approx};
use crate::types::{Type, TypeId};

/// The class whose type parameters are replaced, and their arguments.
#[derive(Clone, Copy)]
struct Subst<'a> {
    class: SymbolId,
    args: &'a [TypeId],
}

impl Universe {
    /// `t` with the type parameters of class `c` replaced by `args`. A wildcard argument gives
    /// its upper bound where the parameter occurs covariantly, its lower bound contravariantly,
    /// and a wildcard of those bounds as an invariant type argument: the base type `Box[Any]` of
    /// `Sub[?]` for `class Sub[+A] extends Box[A]` (dotty's `substApprox`).
    pub fn subst_approx(&mut self, t: TypeId, c: SymbolId, args: &[TypeId]) -> TypeId {
        let subst = Subst { class: c, args };
        if args.iter().any(|&a| matches!(self.ty(a), Type::Bounds(..))) {
            self.approximated(t, 1, subst).hi()
        } else {
            self.substituted(t, subst)
        }
    }

    fn substituted(&mut self, t: TypeId, s: Subst<'_>) -> TypeId {
        self.map_type(t, &mut |u, x| match u.class_param(x) {
            Some((owner, i)) if owner == s.class => s.args.get(i).copied(),
            _ => None,
        })
    }

    /// `t` at variance `v` with the parameters replaced, a range where a wildcard stays open;
    /// a form that ranges do not reach, such as a selection on a parameter, is substituted as is.
    fn approximated(&mut self, t: TypeId, v: i8, s: Subst<'_>) -> Approx {
        if let Some((owner, i)) = self.class_param(t)
            && owner == s.class
            && let Some(&arg) = s.args.get(i)
        {
            return match *self.ty(arg) {
                Type::Bounds(lo, hi) => approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v),
                _ => Approx::Exact(arg),
            };
        }
        match *self.ty(t) {
            Type::Applied(tycon, args) => {
                let c = self.approximated(tycon, v, s);
                let args = self.types_of(args).to_vec();
                let variances = self.tycon_variances(tycon, args.len());
                let mut mapped = Vec::with_capacity(args.len());
                for (i, &arg) in args.iter().enumerate() {
                    let wildcard = matches!(self.ty(arg), Type::Bounds(..) | Type::Wildcard(_));
                    let pv = variances.get(i).copied().unwrap_or(0);
                    mapped.push(self.approximated(arg, if wildcard { v } else { v * pv }, s));
                }
                approx::applied(self, c, &mapped, &variances, v)
            }
            Type::Alias(x) => {
                let x = self.approximated(x, 0, s);
                approx::alias(self, x, v)
            }
            Type::Bounds(lo, hi) => {
                let lo = self.approximated(lo, -v, s);
                let hi = self.approximated(hi, v, s);
                approx::bounds(self, lo, hi, v)
            }
            Type::Refined(parent, name, info) => {
                let parent = self.approximated(parent, v, s);
                let info = self.approximated(info, v, s);
                approx::refined(self, parent, name, info, v)
            }
            Type::And(a, b) => {
                let parts = [self.approximated(a, v, s), self.approximated(b, v, s)];
                let ranged = parts.iter().any(|p| p.is_range());
                approx::derived(self, &parts, &[], v, &mut |u, p| match ranged {
                    true => u.glb(p[0], p[1]),
                    false => u.intern_type(Type::And(p[0], p[1])),
                })
            }
            Type::Or(a, b, soft) => {
                let parts = [self.approximated(a, v, s), self.approximated(b, v, s)];
                let ranged = parts.iter().any(|p| p.is_range());
                approx::derived(self, &parts, &[], v, &mut |u, p| match ranged {
                    true => u.lub_as(p[0], p[1], soft),
                    false => u.intern_type(Type::Or(p[0], p[1], soft)),
                })
            }
            Type::Annotated(x, annot) => {
                let x = self.approximated(x, v, s);
                approx::derived(self, &[x], &[], v, &mut |u, p| u.intern_type(Type::Annotated(p[0], annot)))
            }
            _ => Approx::Exact(self.substituted(t, s)),
        }
    }
}
