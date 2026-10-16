//! Approximating references to local symbols away (dotty's `AvoidMap`).

use super::Typer;
use super::approx::{self, Approx};
use tasty::{Kind, NOTHING, SymbolId, Type, TypeId, TypeVars};

/// The symbols to avoid, and those whose bounds are unknown: reaching one fails.
#[derive(Clone, Copy)]
pub struct Avoided<'a> {
    pub locals: &'a [SymbolId],
    pub opaque: &'a [SymbolId],
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
                let p = m(self, pre, v.max(0))?;
                self.avoid_select(t, pre, p, s, v, a)
            }
            Type::This(_) | Type::Sym(..) => exact(t),
            Type::Applied(tycon, args) => {
                let c = m(self, tycon, v)?;
                let variances = match *self.u.types().get(tycon) {
                    Type::Sym(_, s) => self.u.variances(s),
                    _ => Vec::new(),
                };
                let mut mapped = Vec::with_capacity(args.len());
                for (i, &arg) in args.iter().enumerate() {
                    mapped.push(m(self, arg, v * variances.get(i).copied().unwrap_or(0))?);
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
            Type::Match { bound, .. } => {
                if self.refers_to(t, a.locals) {
                    let hi = m(self, bound, v)?;
                    return Some(approx::range(self.u, Approx::Exact(NOTHING), hi, v));
                }
                exact(t)
            }
            _ => exact(t),
        }
    }

    /// A reference to local symbol `s` approximated: a term by its widened type, an alias by its
    /// right-hand side, an abstract type by its bounds, a class by its `class_bound`.
    fn avoid_local(&mut self, s: SymbolId, v: i8, a: Avoided) -> Option<Approx> {
        let info = self.u.info(s);
        let info = match *self.u.types().get(info) {
            Type::ByName(r) => r,
            _ => info,
        };
        match self.u.symbol(s).kind {
            Kind::Class => {
                let bound = self.local_class_bound(s)?;
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
    fn avoid_info(&mut self, info: TypeId, v: i8, a: Avoided) -> Option<Approx> {
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

    /// `pre.s` with prefix `pre` approximated as `p` (dotty's `AvoidMap.derivedSelect`): the
    /// member's alias, bounds or singleton type when it has one, a term's widened type at a
    /// positive variance, the member of the upper prefix otherwise.
    fn avoid_select(
        &mut self,
        t: TypeId,
        pre: TypeId,
        p: Approx,
        s: SymbolId,
        v: i8,
        a: Avoided,
    ) -> Option<Approx> {
        if p == Approx::Exact(pre) {
            return Some(Approx::Exact(t));
        }
        let singleton = matches!(p, Approx::Exact(q) if self.is_path(q));
        if !singleton && let Some(r) = self.try_widen(t, pre, s, v, a) {
            return r;
        }
        let kind = self.u.symbol(s).kind;
        if kind.is_term() && v > 0 && !singleton {
            let info = self.u.term_type(Some(pre), s);
            let info = self.u.widen(info);
            return self.avoid_at(info, v, a);
        }
        let hi = self.u.intern_type(Type::Sym(Some(p.hi()), s));
        let lo = match p.lo() {
            NOTHING => NOTHING,
            lo => self.u.intern_type(Type::Sym(Some(lo), s)),
        };
        Some(approx::range(self.u, Approx::Exact(lo), Approx::Exact(hi), v))
    }

    /// `pre.s` replaced by what it is equal to or bounded by (dotty's `tryWiden` and
    /// `expandParam`); `None` if it has no such type, `Some(None)` if avoiding that fails.
    fn try_widen(
        &mut self,
        t: TypeId,
        pre: TypeId,
        s: SymbolId,
        v: i8,
        a: Avoided,
    ) -> Option<Option<Approx>> {
        match self.u.symbol(s).kind {
            Kind::TypeParam => {
                let arg = self.resolve_class_params(t);
                (arg != t).then(|| match *self.u.types().get(arg) {
                    Type::Bounds(..) => self.avoid_info(arg, v, a),
                    _ => self.avoid_at(arg, v, a),
                })
            }
            Kind::Type => {
                let alias = self.u.dealias(t);
                if alias != t {
                    return Some(self.avoid_at(alias, v, a));
                }
                let info = self.u.member_info(pre, s);
                matches!(self.u.types().get(info), Type::Bounds(..)).then(|| self.avoid_info(info, v, a))
            }
            k if k.is_term() => {
                let info = self.u.term_type(Some(pre), s);
                let info = match *self.u.types().get(info) {
                    Type::ByName(r) => r,
                    _ => info,
                };
                self.is_path(info).then(|| self.avoid_at(info, v, a))
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

    /// Whether `t` is a singleton type: a path, `this`, a constant or a skolem.
    fn is_path(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::Skolem(..) => true,
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            _ => false,
        }
    }
}
