// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! Approximating references to local symbols away (dotty's `AvoidMap`).

use super::Typer;
use super::approx::{self, Approx};
use tasty::{Kind, NOTHING, SymbolId, Type, TypeId, TypeVars};

/// The symbols to avoid, those whose bounds are unknown, and the local symbols being expanded:
/// reaching one of the last two fails.
#[derive(Clone, Copy)]
pub struct Avoided<'a> {
    pub locals: &'a [SymbolId],
    pub opaque: &'a [SymbolId],
    pub visiting: &'a [SymbolId],
}

impl<'a> Avoided<'a> {
    pub fn new(locals: &'a [SymbolId], opaque: &'a [SymbolId]) -> Self {
        Avoided { locals, opaque, visiting: &[] }
    }
}

impl Typer<'_> {
    /// `t` approximated at variance `v` by types without references to `a.locals`; `None` if an
    /// opaque symbol or a local class whose bound is out of the subset is reached.
    pub fn avoid_at(&mut self, t: TypeId, v: i8, a: Avoided) -> Option<Approx> {
        let exact = |t| Some(Approx::Exact(t));
        let m = |this: &mut Self, t, v| this.avoid_at(t, v, a);
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if a.opaque.contains(&s) => None,
            Type::Sym(_, s) if a.locals.contains(&s) => self.avoid_local(s, v, a),
            Type::Sym(Some(pre), s) if !self.u.is_static_symbol(s) => {
                let Some(p) = m(self, pre, v.max(0)) else {
                    return self.avoid_select_on_failed(t, pre, s, v, a);
                };
                self.avoid_select(t, pre, p, s, v, a)
            }
            Type::This(_) | Type::Sym(..) => exact(t),
            Type::Applied(tycon, args) => {
                let c = m(self, tycon, v)?;
                let variances = approx::tycon_variances(self.u, tycon, args.len());
                let mut mapped = Vec::with_capacity(args.len());
                for (&arg, &pv) in args.iter().zip(&variances) {
                    let wildcard = matches!(self.u.types().get(arg), Type::Bounds(..) | Type::Wildcard(_));
                    mapped.push(m(self, arg, if wildcard { v } else { v * pv })?);
                }
                Some(approx::applied(self.u, c, &mapped, &variances, v))
            }
            Type::Alias(x) => {
                let x = m(self, x, 0)?;
                Some(approx::alias(self.u, x, v))
            }
            Type::Bounds(lo, hi) => {
                let lo = m(self, lo, -v)?;
                let hi = m(self, hi, v)?;
                Some(approx::bounds(self.u, lo, hi, v))
            }
            Type::Refined(parent, name, info) => {
                let parent = m(self, parent, v)?;
                let info = m(self, info, v)?;
                Some(approx::refined(self.u, parent, &name, info, v))
            }
            Type::And(x, y) | Type::Or(x, y, _) => {
                let parts = [m(self, x, v)?, m(self, y, v)?];
                let node = self.u.types().get(t).clone();
                Some(approx::derived(self.u, &parts, &[], v, &mut |u, p| match node {
                    Type::Or(_, _, soft) => u.intern_type(Type::Or(p[0], p[1], soft)),
                    _ => u.intern_type(Type::And(p[0], p[1])),
                }))
            }
            Type::ByName(x) => self.avoid_unary(t, x, v, a, Type::ByName),
            Type::Annotated(x, an) => self.avoid_unary(t, x, v, a, |x| Type::Annotated(x, an)),
            Type::Rec(b, x) => self.avoid_unary(t, x, v, a, |x| Type::Rec(b, x)),
            Type::Wildcard(Some(b)) => {
                let b = m(self, b, v)?.to_bounds(self.u);
                exact(self.u.intern_type(Type::Wildcard(Some(b))))
            }
            Type::Skolem(_, info) => match m(self, info, v)? {
                Approx::Exact(i) if i == info => exact(t),
                i => Some(approx::range(self.u, Approx::Exact(NOTHING), i, v)),
            },
            Type::Var(var) => self.avoid_var(t, var, v, a),
            Type::Method { .. } | Type::Poly { .. } | Type::Lambda { .. } => self.avoid_lambda(t, v, a),
            Type::Match { .. } if self.refers_to(t, a.locals) => None,
            Type::Constant(tasty::tags::CLASSconst, _) if self.refers_to(t, a.locals) => {
                let class = self.u.widen(t);
                m(self, class, v)
            }
            _ => exact(t),
        }
    }

    /// A reference to local symbol `s` approximated: a term by its widened type, an alias by its
    /// right-hand side, an abstract type by its bounds, a class by its `class_bound`; `None` if
    /// `s` is reached again while expanding it.
    fn avoid_local(&mut self, s: SymbolId, v: i8, a: Avoided) -> Option<Approx> {
        if a.visiting.contains(&s) {
            return None;
        }
        let visiting = [a.visiting, &[s]].concat();
        let a = Avoided { visiting: &visiting, ..a };
        let info = self.u.info(s);
        let info = match *self.u.types().get(info) {
            Type::ByName(r) => r,
            _ => info,
        };
        match self.u.symbol(s).kind {
            Kind::Class => {
                let bound = self.local_class_bound(s, a.visiting)?;
                let hi = self.avoid_at(bound, v, a)?;
                Some(approx::range(self.u, Approx::Exact(NOTHING), hi, v))
            }
            k if k.is_term() => {
                let info = self.u.dealias(info);
                let hi = self.avoid_at(info, v, a)?;
                if self.is_path(info) {
                    return Some(hi);
                }
                Some(approx::range(self.u, Approx::Exact(NOTHING), hi, v))
            }
            _ => self.avoid_info(info, v, a),
        }
    }

    /// The approximation of a type whose info is `info`: its alias, or its bounds.
    pub fn avoid_info(&mut self, info: TypeId, v: i8, a: Avoided) -> Option<Approx> {
        match *self.u.types().get(info) {
            Type::Alias(x) => self.avoid_at(x, v, a),
            Type::Bounds(lo, hi) => {
                let lo = self.avoid_at(lo, -v, a)?;
                let hi = self.avoid_at(hi, v, a)?;
                Some(approx::range(self.u, lo, hi, v))
            }
            _ => None,
        }
    }

    /// `make(x)` with its single part `x` approximated.
    fn avoid_unary(
        &mut self,
        t: TypeId,
        x: TypeId,
        v: i8,
        a: Avoided,
        make: impl Fn(TypeId) -> Type,
    ) -> Option<Approx> {
        let mapped = self.avoid_at(x, v, a)?;
        if mapped == Approx::Exact(x) {
            return Some(Approx::Exact(t));
        }
        Some(approx::derived(self.u, &[mapped], &[], v, &mut |u, p| u.intern_type(make(p[0]))))
    }

    /// Uninstantiated type variable `t` replaced by the bound it would be instantiated to if
    /// avoiding changes that bound (dotty's `avoid` of a `TypeVar`).
    fn avoid_var(&mut self, t: TypeId, var: tasty::VarId, v: i8, a: Avoided) -> Option<Approx> {
        if let Some(inst) = self.constraint.instance(var) {
            return self.avoid_at(inst, v, a);
        }
        let (lo, hi) = self.constraint.bounds(var);
        let bound = if v > 0 || v == 0 && lo != NOTHING { lo } else { hi };
        let bound = self.zonk(bound);
        let mapped = self.avoid_at(bound, v, a)?;
        Some(if mapped == Approx::Exact(bound) { Approx::Exact(t) } else { mapped })
    }

    /// A method, polymorphic or lambda type with its parameters approximated contravariantly
    /// and its result covariantly (dotty's `derivedLambdaType`).
    fn avoid_lambda(&mut self, t: TypeId, v: i8, a: Avoided) -> Option<Approx> {
        let (params, result) = match self.u.types().get(t) {
            Type::Method { params, result, .. }
            | Type::Poly { params, result, .. }
            | Type::Lambda { params, result, .. } => (params.clone(), *result),
            _ => return Some(Approx::Exact(t)),
        };
        let mut parts = Vec::with_capacity(params.len() + 1);
        for (_, p) in &params {
            parts.push(self.avoid_at(*p, -v, a)?);
        }
        parts.push(self.avoid_at(result, v, a)?);
        let flip: Vec<bool> = (0..parts.len()).map(|i| i < params.len()).collect();
        let node = self.u.types().get(t).clone();
        Some(approx::derived(self.u, &parts, &flip, v, &mut |u, p| {
            let (ps, r) = p.split_at(params.len());
            let ps = params.iter().zip(ps).map(|((n, _), &t)| (n.clone(), t)).collect();
            u.intern_type(match node.clone() {
                Type::Method { binder, implicit, contextual, .. } => {
                    Type::Method { binder, params: ps, result: r[0], implicit, contextual }
                }
                Type::Poly { binder, .. } => Type::Poly { binder, params: ps, result: r[0] },
                Type::Lambda { binder, .. } => Type::Lambda { binder, params: ps, result: r[0] },
                other => other,
            })
        }))
    }
}
