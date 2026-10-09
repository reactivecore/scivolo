//! Choosing among the alternatives a name denotes by the shape of an application: values
//! without `apply` dropped, methods by the number of arguments they take (dotty's
//! `narrowBySize` with `sizeFits`), before overload resolution.

use super::{Pt, Typer};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The only alternative of `syms` without parameter lists, its type conforming to `pt`
    /// unless that is a function type (dotty's `resolveOverloaded` without arguments).
    fn parameterless_alternative(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        pt: Pt,
    ) -> Result<SymbolId, &'static str> {
        if let Pt::Type(p) = pt
            && let Some((args, _)) = (0..=22).find_map(|n| self.function_parts(p, n))
        {
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
            return match fitting[..] {
                [s] => Ok(s),
                _ => Err("overloaded reference against a function type"),
            };
        }
        let mut found = Vec::new();
        for &s in syms {
            self.complete(s);
            let tpe = self.ref_type(prefix, s);
            let info = self.method_info(tpe).unwrap_or(tpe);
            if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
                continue;
            }
            let result = match *self.u.types().get(info) {
                Type::ByName(r) => r,
                _ => info,
            };
            if let Pt::Type(p) = pt
                && !self.u.is_subtype(result, p)
            {
                continue;
            }
            found.push(s);
        }
        match found[..] {
            [s] => Ok(s),
            _ => Err("overloaded reference"),
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
        if let Pt::PolyFun = pt {
            let mut poly = Vec::new();
            for &s in syms {
                self.complete(s);
                let tpe = self.ref_type(prefix, s);
                let info = self.method_info(tpe).unwrap_or(tpe);
                if matches!(self.u.types().get(info), Type::Poly { .. }) {
                    poly.push(s);
                }
            }
            return match poly[..] {
                [s] => Ok(s),
                _ => Err("overloaded reference with type arguments"),
            };
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
        let n = self.protos[proto].args.len();
        let mut fitting = Vec::new();
        for s in methods {
            let info = self.u.info(s);
            let Some((implicit, ps)) = self.u.param_clauses(info).into_iter().next() else { continue };
            if implicit {
                return Err("overloads with a leading implicit parameter clause");
            }
            let dependent = ps.iter().any(|&p| {
                let mut found = false;
                self.u.map_type(p, &mut |u, part| {
                    found |= matches!(u.types().get(part), Type::Param(..));
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
