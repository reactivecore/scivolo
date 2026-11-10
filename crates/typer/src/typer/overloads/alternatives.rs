// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Choosing among the alternatives a name denotes by the shape of an application: values
//! without `apply` dropped, methods by the number of arguments they take (dotty's
//! `narrowBySize` with `sizeFits`), before overload resolution.

use super::applies::ApplyMember;
use super::mapped::leading_implicit;
use super::{NO_SIZE, Resolution, Unchosen, VALUE_APPLY};
use crate::typer::{Pt, Typer};
use tasty::tags::EXTENSION;
use tasty::{Kind, SymbolId, Type, TypeId};

/// Why a method among parameterless alternatives whose results have `apply` methods, as
/// `def f(x: Int)` beside `def f: Int => Int`, is not resolved.
const RESULT_APPLY: &str = "overloaded method whose result has an apply method";

impl Resolution {
    /// Chooses the alternative of `syms` that the expected type selects: the only one, the only
    /// extension method of an extension call, or the one chosen by the kind of the expected
    /// type: type arguments, arguments, or none.
    pub fn choose(self, t: &mut Typer, syms: &[SymbolId]) -> Result<SymbolId, Unchosen> {
        if let [s] = syms {
            return Ok(*s);
        }
        if t.is_extension_apply(self.pt) {
            let extensions: Vec<SymbolId> =
                syms.iter().copied().filter(|&s| t.u.symbol(s).flags.has(EXTENSION)).collect();
            if extensions.len() < syms.len() {
                return self.choose(t, &extensions);
            }
        }
        match self.pt {
            Pt::PolyFun(poly) => self.choose_poly(t, syms, poly),
            Pt::Any | Pt::Type(_) => Ok(self.parameterless_alternative(t, syms)?),
            Pt::Fun(proto) => self.choose_applied(t, syms, proto),
            Pt::AnyFunction => Err("overloaded reference".into()),
        }
    }

    /// Chooses the alternative of `syms` that the arguments of `proto` select: a value whose
    /// `apply` methods take them, the only method, the methods with or without leading implicit
    /// clauses as the arguments are a `using` clause or not, then those that may take as many
    /// arguments, resolved by their types.
    fn choose_applied(self, t: &mut Typer, syms: &[SymbolId], proto: usize) -> Result<SymbolId, Unchosen> {
        let (mut methods, mut values) = (Vec::new(), Vec::new());
        for &s in syms {
            t.complete(s);
            if t.u.symbol(s).kind == Kind::Def {
                methods.push(s);
                continue;
            }
            let value = t.ref_type(self.prefix, s);
            if !t.type_members(value, "apply").is_empty() {
                values.push(s);
            }
        }
        match values[..] {
            [] => {}
            [v] => {
                if let Some(v) = self.applied_value(t, v, &methods, proto)? {
                    return Ok(v);
                }
            }
            _ => return Err(VALUE_APPLY.into()),
        }
        if let [s] = methods[..] {
            return Ok(s);
        }

        if t.protos[proto].using {
            let implicit: Vec<SymbolId> =
                methods.iter().copied().filter(|&s| leading_implicit(t.u, s).is_some()).collect();
            if implicit.len() < methods.len() {
                return self.choose(t, &implicit);
            }
        } else if methods.iter().any(|&s| leading_implicit(t.u, s) == Some(true)) {
            return self.resolve_without_implicits(t, &methods);
        }

        t.untuple_unless_unary(proto, &methods)?;
        let n = t.protos[proto].args.len();
        let (fitting, applies) = by_size(t, methods, n)?;
        let chosen = match fitting.as_slice() {
            [s] => *s,
            [] if applies.is_empty() => return Err(NO_SIZE.into()),
            [] => return Err(RESULT_APPLY.into()),
            _ => self.resolve_overloaded(t, &fitting, proto)?,
        };
        if !applies.is_empty() && !self.wins_over_applies(t, chosen, &applies, proto) {
            return Err(RESULT_APPLY.into());
        }
        Ok(chosen)
    }
}

/// Finds the methods of `methods` whose first clause may take `n` arguments, and the `apply`
/// members that may of the results of parameterless ones, as `def f: Int => Int` (dotty's
/// `narrowBySize`); refuses methods whose parameter types depend on each other.
fn by_size(
    t: &mut Typer,
    methods: Vec<SymbolId>,
    n: usize,
) -> Result<(Vec<SymbolId>, Vec<ApplyMember>), &'static str> {
    let mut fitting = Vec::new();
    let mut applies = Vec::new();
    for s in methods {
        let info = t.u.info(s);
        let Some((_, ps)) = t.u.param_clauses(info).into_iter().next() else {
            applies.extend(result_applies(t, info, n));
            continue;
        };
        let method = t.u.strip_poly(info);
        let Some(binder) = t.u.method(method).map(|m| m.binder) else { continue };
        let dependent = ps.iter().any(|&p| {
            let mut found = false;
            t.u.map_type(p, &mut |u, part| {
                found |= u.param_ref(part).is_some_and(|(b, _)| b == binder);
                None
            });
            found
        });
        if dependent {
            return Err("overloads with dependent parameters");
        }
        if size_fits(t, s, &ps, n) {
            fitting.push(s);
        }
    }
    Ok((fitting, applies))
}

/// Finds the `apply` members of the result of parameterless method type `info` whose first
/// clause may take `n` arguments.
fn result_applies(t: &mut Typer, info: TypeId, n: usize) -> Vec<ApplyMember> {
    let value = match *t.u.ty(info) {
        Type::Poly { result, .. } | Type::ByName(result) => result,
        _ => info,
    };
    let mut applies = Vec::new();
    for apply in t.type_members(value, "apply") {
        t.complete(apply);
        let apply_info = t.u.info(apply);
        if let Some((_, aps)) = t.u.param_clauses(apply_info).into_iter().next()
            && size_fits(t, apply, &aps, n)
        {
            applies.push(ApplyMember { apply, value, info });
        }
    }
    applies
}

/// Checks whether method `s` of first-clause parameter types `ps` may take `n` arguments,
/// missing ones by default or as an empty vararg (dotty's `sizeFits`).
fn size_fits(t: &mut Typer, s: SymbolId, ps: &[TypeId], n: usize) -> bool {
    let varargs = ps.last().is_some_and(|&p| t.repeated_elem(p).is_some());
    if ps.len() <= n {
        return ps.len() == n || varargs;
    }
    let defaults = t.default_params(s).iter().filter(|&&d| d).count();
    let necessary = ps.len().saturating_sub(defaults);
    necessary <= n || necessary == n + 1 && varargs
}
