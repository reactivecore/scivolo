// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Whether an alternative applies to arguments of given types (dotty's `isApplicableMethodRef`,
//! `isCompatible`): by subtyping and numeric widening, closures by their single abstract method,
//! and with implicit conversions where nothing applies without.

use super::Typer;
use super::implicits::conversions::Goal;
use crate::ctx::Ctx;
use crate::tpd::Tree as T;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether an argument of type `arg` fits a parameter of type `param` in `ctx`, through an
    /// implicit conversion if `views` (dotty's `isCompatible`, else its direct applicability):
    /// `None` if undecided.
    pub fn compatible(&mut self, arg: TypeId, param: TypeId, views: bool, ctx: Ctx) -> Option<bool> {
        if self.conforms(arg, param) {
            return Some(true);
        }
        match self.sam_arg_ok(arg, param, views) {
            Ok(Some(fits)) => return Some(fits),
            Ok(None) => {}
            Err(_) => return None,
        }
        if let (Some(a), Some(p)) = (self.value_class(arg), self.value_class(param))
            && super::widening::widens(&a, &p)
        {
            return Some(true);
        }
        if !views {
            return Some(false);
        }
        self.view_exists(arg, param, ctx)
    }

    /// Whether an implicit conversion takes a value of type `arg` to `param` (dotty's
    /// `viewExists`): `None` if the search is outside the subset.
    fn view_exists(&mut self, arg: TypeId, param: TypeId, ctx: Ctx) -> Option<bool> {
        let (before, unsupported) = (self.constraint.attempt(), self.unsupported.clone());
        let dummy = self.tpd.node(T::TypeTree { inferred: true }, syntax::ast::Span::NONE, Some(arg), None);
        let found = self.converted(dummy, Goal::Conform(param), ctx);
        let clean = self.unsupported == unsupported;
        self.constraint.abandon(before);
        self.unsupported = unsupported;
        match found {
            Ok(found) if clean => Some(found.is_some()),
            _ => None,
        }
    }

    /// Whether an argument of function type `arg` fits single abstract method type `param`: `arg`
    /// conforms to the function type of the method (dotty's `SAMargOK`); `None` if `arg` is no
    /// function type or `param` no such type, or a `PartialFunction` unless `views`.
    fn sam_arg_ok(&mut self, arg: TypeId, param: TypeId, views: bool) -> Result<Option<bool>, &'static str> {
        let arg = self.u.widen(arg);
        let Some(n) = (0..=22).find(|&n| self.function_parts(arg, n).is_some()) else { return Ok(None) };
        if self.function_parts(param, n).is_some() || !views && self.is_partial_function(param) {
            return Ok(None);
        }
        let Some(sam) = self.sam_parts(param, n, false)? else { return Ok(None) };
        let class = self.function_class(n).ok_or("function class")?;
        let mut args = sam.params;
        args.push(sam.result);
        let fun = self.u.intern_type(Type::Applied(class, args));
        Ok(Some(self.conforms(arg, fun)))
    }

    /// Whether arguments of types `args` fit the first parameter clause of `alt` seen from
    /// `prefix` in `ctx`, through implicit conversions if `views`: `None` if undecided.
    pub fn applicable(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        args: &[TypeId],
        views: bool,
        ctx: Ctx,
    ) -> Option<bool> {
        let before = self.constraint.attempt();
        let fits = self.applicable_with_vars(alt, prefix, args, views, ctx);
        self.constraint.abandon(before);
        fits
    }

    fn applicable_with_vars(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        args: &[TypeId],
        views: bool,
        ctx: Ctx,
    ) -> Option<bool> {
        let (mut params, repeated) = self.param_types(alt, prefix, true)?;
        let elem = repeated.then(|| params.pop()).flatten();
        if args.len() > params.len() && elem.is_none() {
            return Some(false);
        }
        if args.len() < params.len() {
            let defaults = self.default_params(alt);
            if !(args.len()..params.len()).all(|i| defaults.get(i) == Some(&true)) {
                return Some(false);
            }
        }
        let mut all = true;
        for (i, &a) in args.iter().enumerate() {
            let p = params.get(i).copied().or(elem)?;
            let by_name_result = |t: &Self, x: TypeId| match *t.u.types().get(x) {
                Type::ByName(r) => r,
                _ => x,
            };
            let p = self.from_java_object(Some(alt), p);
            let (a, p) = (by_name_result(self, a), by_name_result(self, p));
            all &= self.compatible(a, p, views, ctx)?;
        }
        Some(all)
    }

    /// `param_types` of `alt` with its type parameters, if any, replaced by fresh abstract types
    /// within their bounds (dotty's `newTypeParams` in `isAsGood`).
    pub fn rigid_param_types(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
    ) -> Option<(Vec<TypeId>, bool)> {
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let info = self.method_info(tpe)?;
        if !matches!(self.u.types().get(info), Type::Poly { .. }) {
            return self.param_types(alt, prefix, false);
        }
        let info = self.poly_instance(info, alt, true);
        let Type::Method { params, .. } = self.u.types().get(info).clone() else { return None };
        let mut types: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
        let elem = types.last().and_then(|&p| self.repeated_elem(p));
        if let Some(e) = elem {
            *types.last_mut().expect("a parameter") = e;
        }
        Some((types, elem.is_some()))
    }

    /// The parameter types of the first clause of `alt` seen from `prefix`, a repeated one as its
    /// element type, and whether the last is repeated; of a polymorphic `alt` only if `poly`,
    /// its type parameters replaced by fresh type variables within their bounds.
    pub fn param_types(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        poly: bool,
    ) -> Option<(Vec<TypeId>, bool)> {
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let mut info = self.method_info(tpe)?;
        if matches!(self.u.types().get(info), Type::Poly { .. }) {
            if !poly {
                return None;
            }
            info = self.poly_instance(info, alt, false);
        }
        let Type::Method { params, .. } = self.u.types().get(info).clone() else { return None };
        let mut types: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
        let elem = types.last().and_then(|&p| self.repeated_elem(p));
        if let Some(e) = elem {
            *types.last_mut().expect("a parameter") = e;
        }
        Some((types, elem.is_some()))
    }

    /// The result of polymorphic type `info` of `owner`, its type parameters replaced by fresh
    /// abstract types within their bounds if `rigid`, else by fresh type variables within them;
    /// `info` itself if not polymorphic.
    pub fn poly_instance(&mut self, info: TypeId, owner: SymbolId, rigid: bool) -> TypeId {
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else { return info };
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        if let Some(args) = self.mapped_targs_of(owner, names.len()) {
            return self.instantiate_poly(result, binder, &names, &args, Some(owner));
        }
        let mut args = Vec::new();
        for n in &names {
            args.push(if rigid {
                let s = self.syms.define(self.u, n, Kind::TypeParam, owner, syntax::ast::flags::TYPE_PARAM);
                self.u.unenter(s);
                self.u.intern_type(Type::Sym(None, s))
            } else {
                self.constraint.fresh(self.u)
            });
        }
        for (&(_, bounds), &a) in params.iter().zip(&args) {
            let bounds = self.instantiate_poly(bounds, binder, &names, &args, Some(owner));
            if let Type::Sym(_, s) = *self.u.types().get(a) {
                self.u.set_info(s, bounds);
            } else if let Type::Bounds(lo, hi) = *self.u.types().get(bounds) {
                self.conforms(lo, a);
                self.conforms(a, hi);
            }
        }
        self.instantiate_poly(result, binder, &names, &args, Some(owner))
    }
}
