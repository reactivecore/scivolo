// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Substituters.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala

mod match_types;
mod select;

use super::{SymbolId, Universe};
use crate::approx::{self, Approx};
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING};

/// The class whose type parameters are replaced, and their arguments.
#[derive(Clone, Copy)]
pub struct Subst<'a> {
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
            self.map_type(t, &mut |u, x| match u.class_param(x) {
                Some((owner, i)) if owner == c => args.get(i).copied(),
                _ => None,
            })
        }
    }

    /// `t` at variance `v` with the parameters replaced, a range where a wildcard stays open
    /// (dotty's `SubstApproxMap`).
    fn approximated(&mut self, t: TypeId, v: i8, s: Subst<'_>) -> Approx {
        if let Some((owner, i)) = self.class_param(t)
            && owner == s.class
            && let Some(&arg) = s.args.get(i)
        {
            return self.approx_arg(arg, v);
        }
        match *self.ty(t) {
            Type::Sym(Some(p), _) | Type::TypeRef(p, _) | Type::TermRef(p, _) => {
                let pre = self.approximated(p, v, s);
                self.derived_select(t, p, pre, v, s)
            }
            Type::Applied(tycon, args) => self.approximated_applied(t, tycon, args, v, s),
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
                    false if p[0] == p[1] => p[0],
                    false => u.intern_type(Type::And(p[0], p[1])),
                })
            }
            Type::Or(a, b, soft) => {
                let parts = [self.approximated(a, v, s), self.approximated(b, v, s)];
                let ranged = parts.iter().any(|p| p.is_range());
                approx::derived(self, &parts, &[], v, &mut |u, p| match ranged {
                    true => u.lub_as(p[0], p[1], soft),
                    false if p[0] == p[1] => p[0],
                    false => u.intern_type(Type::Or(p[0], p[1], soft)),
                })
            }
            Type::Annotated(x, annot) => self.approximated_unary(x, v, s, |x| Type::Annotated(x, annot)),
            Type::ByName(x) => self.approximated_unary(x, v, s, Type::ByName),
            Type::Rec(b, x) => self.approximated_unary(x, v, s, |x| Type::Rec(b, x)),
            Type::Flexible(x) => match self.approximated(x, v, s) {
                Approx::Exact(x) => Approx::Exact(self.flexible(x)),
                Approx::Range(lo, hi) => {
                    let lo = self.or_null(lo);
                    approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v)
                }
            },
            Type::Method { params, result, .. }
            | Type::Poly { params, result, .. }
            | Type::Lambda { params, result, .. } => {
                let pv = if matches!(self.ty(result), Type::MatchCase(..)) { 0 } else { -v };
                let params = self.params_of(params).to_vec();
                let params: Vec<_> = params.iter().map(|&(_, p)| self.approximated(p, pv, s)).collect();
                let result = self.approximated(result, v, s);
                approx::lambda(self, t, &params, result, v)
            }
            Type::Super(this, parent) => {
                let parts = [self.approximated(this, v, s), self.approximated(parent, v, s)];
                match parts {
                    [Approx::Exact(this), Approx::Exact(parent)] => {
                        Approx::Exact(self.intern_type(Type::Super(this, parent)))
                    }
                    _ => approx::empty(self, v),
                }
            }
            Type::Match { bound, scrutinee, cases } => self.approximated_match(bound, scrutinee, cases, v, s),
            Type::MatchCase(pat, body) => {
                let parts = [self.approximated(pat, 0, s), self.approximated(body, v, s)];
                approx::derived(self, &parts, &[], v, &mut |u, p| u.intern_type(Type::MatchCase(p[0], p[1])))
            }
            Type::Skolem(_, info) => match self.approximated(info, v, s) {
                Approx::Exact(i) if i == info => Approx::Exact(t),
                i => approx::range(self, Approx::Exact(NOTHING), i, v),
            },
            Type::Wildcard(Some(b)) => {
                let b = self.approximated(b, v, s).to_bounds(self);
                Approx::Exact(self.intern_type(Type::Wildcard(Some(b))))
            }
            _ => Approx::Exact(t),
        }
    }

    /// The argument `arg` of a parameter: the range of a wildcard's bounds, else `arg`.
    fn approx_arg(&mut self, arg: TypeId, v: i8) -> Approx {
        match *self.ty(arg) {
            Type::Bounds(lo, hi) => approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v),
            _ => Approx::Exact(arg),
        }
    }

    fn approximated_unary(
        &mut self,
        x: TypeId,
        v: i8,
        s: Subst<'_>,
        make: impl Fn(TypeId) -> Type,
    ) -> Approx {
        let x = self.approximated(x, v, s);
        approx::derived(self, &[x], &[], v, &mut |u, p| u.intern_type(make(p[0])))
    }

    /// `tycon[args]` mapped; a constructor that became a type lambda is applied, one that became
    /// `Nothing` or `Any` stays alone.
    fn approximated_applied(
        &mut self,
        t: TypeId,
        tycon: TypeId,
        args: crate::types::TypeList,
        v: i8,
        s: Subst<'_>,
    ) -> Approx {
        let c = self.approximated(tycon, v, s);
        let args = self.types_of(args).to_vec();
        let variances = self.tycon_variances(tycon, args.len());
        let mut mapped = Vec::with_capacity(args.len());
        for (i, &arg) in args.iter().enumerate() {
            let wildcard = matches!(self.ty(arg), Type::Bounds(..) | Type::Wildcard(_));
            let pv = variances.get(i).copied().unwrap_or(0);
            mapped.push(self.approximated(arg, if wildcard { v } else { v * pv }, s));
        }
        let applied = approx::applied(self, c, &mapped, &variances, v);
        if c == Approx::Exact(tycon) {
            return applied;
        }
        match applied {
            Approx::Exact(x) if x == t => applied,
            Approx::Exact(x) => Approx::Exact(self.reduced_application(x)),
            Approx::Range(lo, hi) => {
                let (lo, hi) = (self.reduced_application(lo), self.reduced_application(hi));
                approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v)
            }
        }
    }

    fn reduced_application(&mut self, t: TypeId) -> TypeId {
        let Some((tycon, _)) = self.applied(t) else { return t };
        if tycon == NOTHING || tycon == ANY {
            return tycon;
        }
        if self.lambda(tycon).is_some() { self.beta_reduced(t) } else { t }
    }
}
