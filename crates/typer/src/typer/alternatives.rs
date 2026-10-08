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
            let Some((_, ps)) = self.u.param_clauses(info).into_iter().next() else { continue };
            if ps.iter().any(|&p| self.is_special_param(p)) || self.has_defaults(s) {
                return Err("overloads with default or by-name parameters");
            }
            let repeated = ps.last().is_some_and(|&p| self.repeated_elem(p).is_some());
            let fits = if repeated { n + 1 >= ps.len() } else { n == ps.len() };
            if fits {
                fitting.push(s);
            }
        }
        match fitting.as_slice() {
            [s] => Ok(*s),
            [] => Err("no alternative with that many arguments"),
            _ => self.resolve_overloaded(&fitting, prefix, proto),
        }
    }

    /// Whether method `s` takes type or value parameters.
    pub fn has_params(&mut self, s: SymbolId) -> bool {
        self.complete(s);
        let info = self.u.info(s);
        matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. })
    }

    /// Whether method `s` or a method it overrides has default arguments (dotty's
    /// `hasDefaultParams`).
    pub fn has_defaults(&mut self, s: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(s).owner else { return false };
        if self.declares_defaults(owner, s) {
            return true;
        }
        if self.u.symbol(owner).kind != Kind::Class {
            return false;
        }
        let name = self.u.symbol(s).name.clone();
        let site = crate::defn::type_ref(self.u, owner);
        for base in self.u.linearization(owner).into_iter().skip(1) {
            for m in self.u.decl(base, &name).unwrap_or_default() {
                if self.same_member(site, m, s) && self.declares_defaults(base, m) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether `owner` declares default getters of method `s`, a member of it.
    fn declares_defaults(&mut self, owner: SymbolId, s: SymbolId) -> bool {
        let prefix = format!("{}$default$", self.u.symbol(s).name);
        let decls = self.u.decls(owner).unwrap_or_default();
        decls.iter().any(|&d| self.u.symbol(d).name.starts_with(&prefix))
    }

    /// Whether parameter type `p` is by-name or refers to a parameter.
    pub fn is_special_param(&mut self, p: TypeId) -> bool {
        let mut special = false;
        self.u.map_type(p, &mut |u, part| {
            special |= matches!(u.types().get(part), Type::ByName(_) | Type::Param(..));
            None
        });
        special
    }
}
