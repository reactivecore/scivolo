// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! Approximating references to local symbols away (dotty's `AvoidMap`).

use super::{Avoid, is_path};
use crate::typer::Typer;
use crate::typer::approx::{self, Approx};
use tasty::{Kind, NOTHING, SymbolId, Type, TypeId, TypeVars};

impl Avoid<'_> {
    /// Approximates `tpe` by types that no longer mention the definitions going out of scope:
    /// from above where the type occurs covariantly (`v` 1), from below contravariantly, by both
    /// where invariantly. Gives up on a type that mentions an opaque type whose bounds are
    /// unknown, or a local class whose members the subset cannot describe.
    pub fn at(self, t: &mut Typer, tpe: TypeId, v: i8) -> Option<Approx> {
        let exact = |x| Some(Approx::Exact(x));
        let m = |t: &mut Typer, x, v| self.at(t, x, v);
        let node = *t.u.ty(tpe);
        match node {
            Type::Sym(_, s) if self.own == Some(s) => Some(approx::empty(t.u, v)),
            Type::Sym(_, s) if self.opaque.contains(&s) => None,
            Type::Sym(_, s) if self.locals.contains(&s) => self.local(t, s, v),
            Type::Sym(Some(pre), s) if !t.u.is_static_symbol(s) => {
                let Some(p) = m(t, pre, v.max(0)) else {
                    return self.select_on_failed(t, tpe, pre, s, v);
                };
                self.select(t, tpe, pre, p, s, v)
            }
            Type::This(_) | Type::Sym(..) => exact(tpe),
            Type::TypeRef(pre, _) | Type::TermRef(pre, _) if t.refers_to(pre, self.locals) => {
                self.named(t, tpe, pre, v)
            }
            Type::Applied(tycon, _)
                if t.u.referenced(tycon).is_some_and(|s| self.locals.contains(&s))
                    && t.u.dealias(tpe) != tpe =>
            {
                let dealiased = t.u.dealias(tpe);
                m(t, dealiased, v)
            }
            Type::Applied(tycon, args) => {
                let c = m(t, tycon, v)?;
                let variances = t.u.tycon_variances(tycon, t.u.types_of(args).len());
                let mut mapped = Vec::with_capacity(variances.len());
                for (i, &pv) in variances.iter().enumerate() {
                    let arg = t.u.types_of(args)[i];
                    let wildcard = matches!(t.u.ty(arg), Type::Bounds(..) | Type::Wildcard(_));
                    mapped.push(m(t, arg, if wildcard { v } else { v * pv })?);
                }
                Some(approx::applied(t.u, c, &mapped, &variances, v))
            }
            Type::Alias(x) => {
                let x = m(t, x, 0)?;
                Some(approx::alias(t.u, x, v))
            }
            Type::Bounds(lo, hi) => {
                let lo = m(t, lo, -v)?;
                let hi = m(t, hi, v)?;
                Some(approx::bounds(t.u, lo, hi, v))
            }
            Type::Refined(parent, name, info) => {
                let parent = m(t, parent, v)?;
                let info = m(t, info, v)?;
                Some(approx::refined(t.u, parent, name, info, v))
            }
            Type::And(x, y) | Type::Or(x, y, _) => {
                let parts = [m(t, x, v)?, m(t, y, v)?];
                let node = *t.u.ty(tpe);
                Some(approx::derived(t.u, &parts, &[], v, &mut |u, p| match node {
                    Type::Or(_, _, soft) => u.intern_type(Type::Or(p[0], p[1], soft)),
                    _ => u.intern_type(Type::And(p[0], p[1])),
                }))
            }
            Type::ByName(x) => self.unary(t, tpe, x, v, Type::ByName),
            Type::Annotated(x, an) => self.unary(t, tpe, x, v, |x| Type::Annotated(x, an)),
            Type::Rec(b, x) => self.unary(t, tpe, x, v, |x| Type::Rec(b, x)),
            Type::Wildcard(Some(b)) => {
                let b = m(t, b, v)?.to_bounds(t.u);
                exact(t.u.intern_type(Type::Wildcard(Some(b))))
            }
            Type::Skolem(_, info) => match m(t, info, v)? {
                Approx::Exact(i) if i == info => exact(tpe),
                i => Some(approx::range(t.u, Approx::Exact(NOTHING), i, v)),
            },
            Type::Var(var) => self.var(t, tpe, var, v),
            Type::Method { .. } | Type::Poly { .. } | Type::Lambda { .. } => self.lambda(t, tpe, v),
            Type::Match { .. } if t.refers_to(tpe, self.locals) => None,
            Type::Constant(tasty::tags::CLASSconst, _) if t.refers_to(tpe, self.locals) => {
                let class = t.u.widen(tpe);
                m(t, class, v)
            }
            _ => exact(tpe),
        }
    }

    /// Approximates a reference to local symbol `s`: a term by its widened type, an alias by its
    /// right-hand side, an abstract type by its bounds, a class by its `class_bound`; `None` if
    /// `s` is reached again while expanding it.
    fn local(self, t: &mut Typer, s: SymbolId, v: i8) -> Option<Approx> {
        if self.visiting.contains(&s) {
            return None;
        }
        let visiting = [self.visiting, &[s]].concat();
        let a = Avoid { visiting: &visiting, in_var_bound: false, ..self.elsewhere() };
        let info = t.u.info(s);
        let info = t.u.by_name(info).unwrap_or(info);
        match t.u.symbol(s).kind {
            Kind::Class => {
                let bound = a.class_bound(t, s)?;
                let hi = a.at(t, bound, v)?;
                Some(approx::range(t.u, Approx::Exact(NOTHING), hi, v))
            }
            k if k.is_term() => {
                let info = t.u.dealias(info);
                let hi = a.at(t, info, v)?;
                if is_path(t.u, info) {
                    return Some(hi);
                }
                Some(approx::range(t.u, Approx::Exact(NOTHING), hi, v))
            }
            _ => a.info(t, info, v),
        }
    }

    /// Approximates a type member by what it stands for: the right-hand side of `type T = R`,
    /// or the bounds of `type T >: L <: H`; gives up on anything else.
    pub fn info(self, t: &mut Typer, info: TypeId, v: i8) -> Option<Approx> {
        match *t.u.ty(info) {
            Type::Alias(x) => self.at(t, x, v),
            Type::Bounds(lo, hi) => {
                let lo = self.at(t, lo, -v)?;
                let hi = self.at(t, hi, v)?;
                Some(approx::range(t.u, lo, hi, v))
            }
            _ => None,
        }
    }

    /// `make(x)` with its single part `x` approximated.
    fn unary(
        self,
        t: &mut Typer,
        tpe: TypeId,
        x: TypeId,
        v: i8,
        make: impl Fn(TypeId) -> Type,
    ) -> Option<Approx> {
        let mapped = self.at(t, x, v)?;
        if mapped == Approx::Exact(x) {
            return Some(Approx::Exact(tpe));
        }
        Some(approx::derived(t.u, &[mapped], &[], v, &mut |u, p| u.intern_type(make(p[0]))))
    }

    /// Uninstantiated type variable `tpe` replaced by its instance type if avoiding changes that
    /// type (dotty's `avoid` of a `TypeVar`).
    fn var(self, t: &mut Typer, tpe: TypeId, var: tasty::VarId, v: i8) -> Option<Approx> {
        if let Some(inst) = t.constraint.instance(var) {
            return self.at(t, inst, v);
        }
        if self.in_var_bound {
            return Some(Approx::Exact(tpe));
        }
        let (lo, hi) = t.constraint.bounds(var);
        let from_below = v > 0 || v == 0 && lo != NOTHING;
        let bound = match t.instance_type(var, from_below) {
            Ok(inst) => inst,
            Err(_) => t.zonk(if from_below { lo } else { hi }),
        };
        let mapped = Avoid { in_var_bound: true, ..self }.at(t, bound, v)?;
        Some(if mapped == Approx::Exact(bound) { Approx::Exact(tpe) } else { mapped })
    }

    /// A method, polymorphic or lambda type with its parameters approximated contravariantly
    /// and its result covariantly (dotty's `derivedLambdaType`).
    fn lambda(self, t: &mut Typer, tpe: TypeId, v: i8) -> Option<Approx> {
        let (params, result) = match *t.u.ty(tpe) {
            Type::Method { params, result, .. }
            | Type::Poly { params, result, .. }
            | Type::Lambda { params, result, .. } => (t.u.params_of(params).to_vec(), result),
            _ => return Some(Approx::Exact(tpe)),
        };
        let mut parts = Vec::with_capacity(params.len() + 1);
        for (_, p) in &params {
            parts.push(self.at(t, *p, -v)?);
        }
        parts.push(self.at(t, result, v)?);
        let flip: Vec<bool> = (0..parts.len()).map(|i| i < params.len()).collect();
        let node = *t.u.ty(tpe);
        Some(approx::derived(t.u, &parts, &flip, v, &mut |u, p| {
            let (ps, r) = p.split_at(params.len());
            let ps: Vec<_> = params.iter().zip(ps).map(|(&(n, _), &t)| (n, t)).collect();
            let ps = u.intern_params(&ps);
            u.intern_type(match node {
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
