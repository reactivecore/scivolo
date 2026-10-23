// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Choosing among the alternatives a name denotes by the shape of an application: values
//! without `apply` dropped, methods by the number of arguments they take (dotty's
//! `narrowBySize` with `sizeFits`), before overload resolution.

use super::{Pt, Typer};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The only alternative of `syms` without parameter lists, else the only one whose leading
    /// clause is implicit, its type conforming to `pt` unless that is a function type, else for
    /// a single abstract method type `pt` the only method taking its parameters (dotty's
    /// `resolveOverloaded` without arguments).
    fn parameterless_alternative(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        pt: Pt,
    ) -> Result<SymbolId, &'static str> {
        if let Pt::Type(p) = pt
            && let Some((args, _)) = (0..=22).find_map(|n| self.function_parts(p, n))
        {
            return self.taking_params(syms, prefix, &args);
        }
        let (mut found, mut implicit_found) = (Vec::new(), Vec::new());
        for &s in syms {
            self.complete(s);
            let tpe = self.ref_type(prefix, s);
            let info = self.method_info(tpe).unwrap_or(tpe);
            let info = self.mapped_info(info, s);
            let (result, implicit) = match self.u.types().get(info).clone() {
                Type::ByName(r) => (r, false),
                Type::Poly { result, .. } => match self.u.types().get(result) {
                    Type::Method { params, implicit: false, .. } if !params.is_empty() => continue,
                    _ => return Err("overloaded reference with a polymorphic alternative"),
                },
                Type::Method { params, result, implicit: false, .. } if params.is_empty() => (result, false),
                Type::Method { result, implicit: true, .. } => (result, true),
                Type::Method { .. } => continue,
                _ => (info, false),
            };
            if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
                continue;
            }
            if let Pt::Type(p) = pt
                && self.is_fully_defined(p)
                && !self.u.is_subtype(result, p)
            {
                continue;
            }
            if implicit { implicit_found.push(s) } else { found.push(s) }
        }
        if found.is_empty() {
            found = implicit_found;
        }
        if let (Pt::Type(p), []) = (pt, &found[..])
            && let Some(sam) = (0..=22).find_map(|n| self.sam_shape(p, n).ok().flatten())
        {
            return self.taking_params(syms, prefix, &sam.params);
        }
        match found[..] {
            [s] => Ok(s),
            _ => Err("overloaded reference"),
        }
    }

    /// The only method of `syms` whose parameters take arguments of types `args` (dotty's
    /// `normalizedCompatible` against a function type and the `SAMType` case of
    /// `resolveOverloaded1`).
    fn taking_params(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        args: &[TypeId],
    ) -> Result<SymbolId, &'static str> {
        let mut fitting = Vec::new();
        for &s in syms {
            let Some((formals, false)) = self.param_types(s, prefix, false) else {
                return Err("overloaded reference against a function type");
            };
            if formals.len() == args.len()
                && args.iter().zip(&formals).all(|(&a, &f)| self.u.is_subtype(a, f))
            {
                fitting.push(s);
            }
        }
        match fitting[..] {
            [s] => Ok(s),
            _ => Err("overloaded reference against a function type"),
        }
    }

    /// The alternative of `syms`, members of `prefix`, that `pt` selects: the only one, the only
    /// method taking as many arguments as `pt` supplies, or the one overload resolution picks.
    pub fn choose(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        pt: Pt,
    ) -> Result<SymbolId, &'static str> {
        if let [s] = syms {
            return Ok(*s);
        }
        if let Pt::PolyFun(poly) = pt {
            return self.choose_poly(syms, prefix, poly);
        }
        if matches!(pt, Pt::Any | Pt::Type(_)) {
            return self.parameterless_alternative(syms, prefix, pt);
        }
        let Pt::Fun(proto) = pt else { return Err("overloaded reference") };
        let mut methods = Vec::new();
        for &s in syms {
            self.complete(s);
            if self.u.symbol(s).kind == Kind::Def {
                methods.push(s);
                continue;
            }
            let value = self.ref_type(prefix, s);
            if !self.type_members(value, "apply").is_empty() {
                return Err("overloaded value with an apply method");
            }
        }
        if let [s] = methods[..] {
            return Ok(s);
        }
        if self.protos[proto].using {
            let implicit: Vec<SymbolId> =
                methods.iter().copied().filter(|&s| self.leading_implicit(s).is_some()).collect();
            if implicit.len() < methods.len() {
                return self.choose(&implicit, prefix, pt);
            }
        } else if methods.iter().any(|&s| self.leading_implicit(s) == Some(true)) {
            return self.resolve_without_implicits(&methods, prefix, pt);
        }
        self.untuple_unless_unary(proto, &methods)?;
        let n = self.protos[proto].args.len();
        let mut fitting = Vec::new();
        for s in methods {
            let info = self.u.info(s);
            let Some((_, ps)) = self.u.param_clauses(info).into_iter().next() else {
                let value = match *self.u.types().get(info) {
                    Type::Poly { result, .. } | Type::ByName(result) => result,
                    _ => info,
                };
                for apply in self.type_members(value, "apply") {
                    self.complete(apply);
                    let apply_info = self.u.info(apply);
                    if let Some((_, aps)) = self.u.param_clauses(apply_info).into_iter().next()
                        && self.size_fits(apply, &aps, n)
                    {
                        return Err("overloaded method whose result has an apply method");
                    }
                }
                continue;
            };
            let method = match *self.u.types().get(info) {
                Type::Poly { result, .. } => result,
                _ => info,
            };
            let Type::Method { binder, .. } = *self.u.types().get(method) else { continue };
            let dependent = ps.iter().any(|&p| {
                let mut found = false;
                self.u.map_type(p, &mut |u, part| {
                    found |= matches!(u.types().get(part), Type::Param(b, _) if *b == binder);
                    None
                });
                found
            });
            if dependent {
                return Err("overloads with dependent parameters");
            }
            if self.size_fits(s, &ps, n) {
                fitting.push(s);
            }
        }
        match fitting.as_slice() {
            [s] => Ok(*s),
            [] => Err("no alternative with that many arguments"),
            _ => self.resolve_overloaded(&fitting, prefix, proto),
        }
    }

    /// Whether method `s` of first-clause parameter types `ps` may take `n` arguments, missing
    /// ones by default or as an empty vararg (dotty's `sizeFits`).
    fn size_fits(&mut self, s: SymbolId, ps: &[TypeId], n: usize) -> bool {
        let varargs = ps.last().is_some_and(|&p| self.repeated_elem(p).is_some());
        if ps.len() <= n {
            return ps.len() == n || varargs;
        }
        let defaults = self.default_params(s).iter().filter(|&&d| d).count();
        let necessary = ps.len().saturating_sub(defaults);
        necessary <= n || necessary == n + 1 && varargs
    }

    /// Whether method `s` takes type or value parameters.
    pub fn has_params(&mut self, s: SymbolId) -> bool {
        self.complete(s);
        let info = self.u.info(s);
        matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. })
    }
}
