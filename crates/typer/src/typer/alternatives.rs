// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Choosing among the alternatives a name denotes by the shape of an application: values
//! without `apply` dropped, methods by the number of arguments they take (dotty's
//! `narrowBySize` with `sizeFits`), before overload resolution.

use super::{Pt, Typer};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
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
        if self.is_extension_apply(pt) {
            let extensions: Vec<SymbolId> = syms
                .iter()
                .copied()
                .filter(|&s| self.u.symbol(s).flags.has(tasty::tags::EXTENSION))
                .collect();
            if extensions.len() < syms.len() {
                return self.choose(&extensions, prefix, pt);
            }
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
        let mut applies = Vec::new();
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
                        applies.push((apply, value, info));
                    }
                }
                continue;
            };
            let method = self.u.strip_poly(info);
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
        let chosen = match fitting.as_slice() {
            [s] => Ok(*s),
            [] if applies.is_empty() => Err("no alternative with that many arguments"),
            [] => Err("overloaded method whose result has an apply method"),
            _ => self.resolve_overloaded(&fitting, prefix, proto),
        }?;
        if !applies.is_empty() && !self.wins_over_applies(chosen, &applies, prefix, proto) {
            return Err("overloaded method whose result has an apply method");
        }
        Ok(chosen)
    }

    /// Whether method `m`, a member of `prefix`, applies to the arguments of `proto` and is
    /// better than the `apply` members `applies` (each with the result type it is a member of and
    /// the type of that result's alternative) by its owner, a module whose companion class
    /// derives from theirs (dotty's `compareOwner`), and as specific as each of them.
    fn wins_over_applies(
        &mut self,
        m: SymbolId,
        applies: &[(SymbolId, TypeId, TypeId)],
        prefix: Option<TypeId>,
        proto: usize,
    ) -> bool {
        let Some(owner) = self.u.symbol(m).owner else { return false };
        let class = match self.u.is_module_class(owner) {
            true => self.u.companion_class(owner),
            false => None,
        };
        for &(a, value, info) in applies {
            let Some(a_owner) = self.u.symbol(a).owner else { return false };
            let derives = |t: &mut Self, c: SymbolId| c != a_owner && t.u.derives_from(c, a_owner);
            let better = derives(self, owner) || class.is_some_and(|c| derives(self, c));
            if !better
                || self.u.derives_from(a_owner, owner)
                || self.u.is_module_class(a_owner)
                || !self.apply_takes_params_of(m, prefix, (a, value, info))
            {
                return false;
            }
        }
        let Ok(arg_types) = self.proto_arg_types(proto, &[m], prefix) else { return false };
        let ctx = self.protos[proto].ctx;
        self.applicable_to_proto(m, prefix, (proto, &arg_types), false, ctx) == Some(true)
    }

    /// Whether `apply`, a member of `value`, the result of an alternative of type `info`, takes
    /// the parameter types of method `m` (dotty's `isAsSpecific(m, apply)`); a type parameter of
    /// `info` bounded by `Any` takes any type, any other part of `info`'s parameters none.
    fn apply_takes_params_of(
        &mut self,
        m: SymbolId,
        prefix: Option<TypeId>,
        (apply, value, info): (SymbolId, TypeId, TypeId),
    ) -> bool {
        let Some((pm, false)) = self.rigid_param_types(m, prefix) else { return false };
        let seen = self.u.member_info(value, apply);
        let seen = self.resolve_class_params(seen);
        let Some((_, pa)) = self.u.param_clauses(seen).into_iter().next() else { return false };
        if pa.len() != pm.len() {
            return false;
        }
        let poly = match self.u.types().get(info) {
            Type::Poly { binder, params, .. } => Some((*binder, params.clone())),
            _ => None,
        };
        for (&p, &a) in pm.iter().zip(&pa) {
            if let Type::Param(b, i) = *self.u.types().get(a)
                && let Some((_, params)) = poly.as_ref().filter(|(pb, _)| *pb == b)
            {
                let Type::Bounds(_, hi) = *self.u.types().get(params[i as usize].1) else { return false };
                if !self.is_exactly_any(hi) {
                    return false;
                }
                continue;
            }
            let mut mentions = false;
            if let Some((binder, _)) = poly {
                self.u.map_type(a, &mut |u, part| {
                    mentions |= matches!(u.types().get(part), Type::Param(b, _) if *b == binder);
                    None
                });
            }
            if mentions || !self.conforms_unlifted(p, a) {
                return false;
            }
        }
        true
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
