// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Choosing among overloaded alternatives by the expected result type (dotty's `adaptByResult`
//! with `resultConforms`).

use super::{AMBIGUOUS, NO_APPLICABLE, Resolution, UNDECIDED_RESULT, Unchosen};
use crate::ctx::Ctx;
use crate::typer::dependent_params::applied_result;
use crate::typer::{Pt, Typer};
use tasty::{NameId, SymbolId, Type, TypeId};

/// What the parameters of an alternative's first clause stand for in its result.
#[derive(Clone, Copy)]
pub enum ParamRefs {
    /// Skolems of their types, a stable type itself (dotty's `TermParamRef`s in `resultConforms`).
    Skolems,
    /// Type variables below their types and `Singleton`, as an application checks its result
    /// (dotty's `resultTypeApprox`).
    TypeVars,
}

impl Resolution {
    /// Checks whether the result of `alt` conforms to the expected result type of `proto`, if
    /// it has one; `None` if undecided.
    pub fn result_fits(self, t: &mut Typer, alt: SymbolId, proto: usize) -> Option<bool> {
        let (Pt::Type(pt), ctx) = (t.protos[proto].pt, t.protos[proto].ctx) else { return Some(true) };
        self.result_conforms(t, alt, pt, (true, ctx), ParamRefs::Skolems)
    }

    /// Checks whether the result of `alt` after its first parameter clause and its implicit
    /// clauses is compatible with `pt` in `ctx`, through an implicit conversion if `views`, its
    /// type parameters made type variables, its parameters standing for `refs`; the constraint
    /// unchanged; `None` if undecided.
    pub fn result_conforms(
        self,
        t: &mut Typer,
        alt: SymbolId,
        pt: TypeId,
        (views, ctx): (bool, Ctx),
        refs: ParamRefs,
    ) -> Option<bool> {
        if t.u.class_symbol(pt) == Some(t.defn.unit) {
            return Some(true);
        }
        let follow = !t.is_context_function_type(pt);
        let before = t.constraint.attempt();
        let tpe = t.u.intern_type(Type::Sym(self.prefix, alt));
        let mut info = t.method_info(tpe).unwrap_or(tpe);
        if let Some((binder, result, names)) = t.u.poly(info).map(|p| (p.binder, p.result, p.names())) {
            let vars: Vec<TypeId> = names.iter().map(|_| t.constraint.fresh(t.u)).collect();
            info = t.instantiate_poly(result, binder, &names, &vars, Some(alt));
        }

        let fits = match t.u.method(info).map(|m| (m.binder, m.params.to_vec(), m.result)) {
            Some((binder, params, result)) => {
                let result = match refs {
                    ParamRefs::Skolems => {
                        let (names, formals): (Vec<NameId>, Vec<TypeId>) = params.into_iter().unzip();
                        applied_result(t.u, result, (binder, &names), Some(alt), &formals)
                    }
                    ParamRefs::TypeVars => {
                        let approx = t.result_type_approx(result, (binder, &params), Some(alt), Pt::Type(pt));
                        approx.map_or(result, |(r, _)| r)
                    }
                };
                match t.proto_normalized((result, Some(alt)), pt, follow, 8) {
                    Some(value) if t.u.is_necessary_subtype_in(value, pt, &mut t.constraint) => Some(true),
                    Some(value) => t.compatible(value, pt, views, ctx),
                    None => None,
                }
            }
            None => Some(true),
        };
        t.constraint.abandon(before);
        fits
    }

    /// Gives `chosen`, or if its result does not conform to the expected result type of
    /// `proto`, the one alternative of `alts` whose result does, or the one resolution picks
    /// among them (dotty's `adaptByResult`); `chosen` if its result is undecided, refused if
    /// another's is.
    pub fn adapted_by_result(
        self,
        t: &mut Typer,
        chosen: SymbolId,
        alts: &[SymbolId],
        proto: usize,
    ) -> Result<SymbolId, Unchosen> {
        if self.result_fits(t, chosen, proto) != Some(false) {
            return Ok(chosen);
        }

        let mut conforming = Vec::new();
        for &a in alts.iter().filter(|&&a| a != chosen) {
            match self.result_fits(t, a, proto) {
                Some(true) => conforming.push(a),
                Some(false) => {}
                None => return Err(UNDECIDED_RESULT.into()),
            }
        }

        Ok(match conforming[..] {
            [] => chosen,
            [alt] => alt,
            _ => match self.resolve_overloaded(t, &conforming, proto) {
                Ok(alt) => alt,
                Err(e) if [NO_APPLICABLE, AMBIGUOUS].contains(&e.what) => chosen,
                Err(e) => return Err(e),
            },
        })
    }
}
