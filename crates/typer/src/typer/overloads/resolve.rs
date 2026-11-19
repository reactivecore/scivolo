// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Choosing among overloaded alternatives by their arguments (dotty's `resolveOverloaded`,
//! `compare`).

use super::by_result::ParamRefs;
use super::named::applicable_to_proto;
use super::{AMBIGUOUS, NO_APPLICABLE, Resolution, UNDECIDED, Unchosen, applicability};
use crate::ctx::Ctx;
use crate::typer::type_ops::TypeOps;
use crate::typer::{Pt, Typer};
use tasty::{SymbolId, Type, TypeId};

impl Resolution {
    /// Chooses the alternative of `alts` that the arguments of `proto` select: of those they
    /// apply to without implicit conversions, else of those they apply to with them, the most
    /// specific one; a tie is broken by the next argument clause, by the alternatives without
    /// default arguments, then by the expected result type, else the application is ambiguous
    /// (dotty's `resolveOverloaded1`).
    pub fn resolve_overloaded(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        proto: usize,
    ) -> Result<SymbolId, Unchosen> {
        let ctx = t.protos[proto].ctx;
        let (applicable, views) = self.applicable_alts(t, alts, proto, ctx)?;

        let Some((&first, rest)) = applicable.split_first() else {
            return Err(NO_APPLICABLE.into());
        };
        let mut best = first;
        for &alt in rest {
            if self.compare_alts(t, (alt, best), views, ctx).ok_or(UNDECIDED)? > 0 {
                best = alt;
            }
        }
        let mut tied = false;
        for &alt in applicable.iter().filter(|&&a| a != best) {
            tied |= self.compare_alts(t, (best, alt), views, ctx).ok_or(UNDECIDED)? <= 0;
        }

        match tied {
            false => Ok(best),
            true => self.break_tie(t, alts, applicable, proto),
        }
    }

    /// Finds the alternatives of `alts` the arguments of `proto` apply to, first without
    /// implicit conversions, then with them, and whether conversions were tried.
    fn applicable_alts(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        proto: usize,
        ctx: Ctx,
    ) -> Result<(Vec<SymbolId>, bool), &'static str> {
        let mut views = false;
        for phase in [false, true] {
            views = phase;
            let narrowed = self.narrow_by_shapes(t, alts, proto, views)?;
            if narrowed.len() < 2 {
                if narrowed.is_empty() {
                    continue;
                }
                return Ok((narrowed, views));
            }

            let arg_types = self.proto_arg_types(t, proto, &narrowed)?;
            let mut applicable = Vec::new();
            for &alt in &narrowed {
                let info = t.u.info(alt);
                t.complete_named_type_members(info, info);
                let seen = t.u.intern_type(Type::Sym(self.prefix, alt));
                if let Some(seen) = t.method_info(seen) {
                    t.complete_named_type_members(seen, seen);
                }
                let fits = applicable_to_proto(t, alt, self.prefix, (proto, &arg_types), views, ctx);
                let fits = match (fits, t.protos[proto].pt) {
                    (Some(true), Pt::Type(pt)) if t.protos[proto].deep => {
                        let refs = ParamRefs::TypeVars;
                        Some(self.result_conforms(t, alt, pt, (views, ctx), refs) != Some(false))
                    }
                    _ => fits,
                };
                match fits {
                    Some(true) => applicable.push(alt),
                    Some(false) => {}
                    None => return Err(UNDECIDED),
                }
            }
            if !applicable.is_empty() {
                return Ok((applicable, views));
            }
        }
        Ok((Vec::new(), views))
    }

    /// Chooses among the equally specific alternatives `applicable` of `alts`: by the arguments
    /// of the next clause, then among `alts` by the only one whose result is no method or the
    /// only one without default arguments, then with the expected result type taken into
    /// account (dotty's `deepenProto`).
    fn break_tie(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        applicable: Vec<SymbolId>,
        proto: usize,
    ) -> Result<SymbolId, Unchosen> {
        match t.protos[proto].pt {
            Pt::Fun(next) => return self.resolve_by_next_clause(t, &applicable, (proto, next)),
            Pt::PolyFun(_) => return Err("ambiguous overload resolved by further arguments".into()),
            _ => {}
        }

        for preferred in [false, true] {
            let kept: Vec<SymbolId> = alts
                .iter()
                .copied()
                .filter(
                    |&a| if preferred { !t.has_defaults(a) } else { !result_is_method(&mut t.types(), a) },
                )
                .collect();
            if let [only] = kept[..] {
                return Ok(only);
            }
            if kept.len() > 1 && kept.len() < alts.len() {
                return self.resolve_overloaded(t, &kept, proto);
            }
        }

        if matches!(t.protos[proto].pt, Pt::Type(_)) && !t.protos[proto].deep {
            t.protos[proto].deep = true;
            let deeper = self.resolve_overloaded(t, alts, proto);
            t.protos[proto].deep = false;
            return deeper;
        }
        Err(Unchosen { what: AMBIGUOUS, ambiguous: Some(applicable) })
    }

    /// Compares alternatives `a` and `b` (dotty's `compare`): 1 if `a` is better, -1 if `b` is, 0
    /// if neither; `None` if undecided.
    fn compare_alts(self, t: &mut Typer, (a, b): (SymbolId, SymbolId), views: bool, ctx: Ctx) -> Option<i32> {
        let (oa, ob) = (t.u.symbol(a).owner?, t.u.symbol(b).owner?);
        let derives = |t: &mut Typer, x, y| i32::from(x != y && t.u.derives_from(x, y));
        let owner_score = derives(t, oa, ob) - derives(t, ob, oa);
        let (pa, va) = applicability::rigid_param_types(t, a, self.prefix)?;
        let (pb, vb) = applicability::rigid_param_types(t, b, self.prefix)?;
        let wins_a = (!va || vb) && applicability::applicable(t, b, self.prefix, &pa, views, ctx)?;
        let wins_b = (!vb || va) && applicability::applicable(t, a, self.prefix, &pb, views, ctx)?;
        Some(match owner_score {
            1 => i32::from(wins_a || !wins_b),
            -1 => -i32::from(wins_b || !wins_a),
            _ => i32::from(wins_a && !wins_b) - i32::from(wins_b && !wins_a),
        })
    }
}

impl Typer<'_> {
    /// Compares members `a` and `b` of `prefix` as overloading resolution does in `ctx` (dotty's
    /// `compare`): 1 if `a` is better, -1 if `b` is, 0 if neither; `None` if undecided.
    pub fn compare_members(&mut self, prefix: TypeId, (a, b): (SymbolId, SymbolId), ctx: Ctx) -> Option<i32> {
        Resolution { prefix: Some(prefix), pt: Pt::Any, ctx, member: None }.compare_alts(self, (a, b), true, ctx)
    }
}

/// Checks whether the first parameter clause of method `s` is followed by another that is not
/// implicit (dotty's `resultIsMethod` with `stripInferrable`), as `def f(x: Int)(y: Int)`.
pub fn result_is_method(types: &mut TypeOps, s: SymbolId) -> bool {
    let info = types.u.info(s);
    let info = types.u.strip_poly(info);
    let Some(result) = types.u.method(info).map(|m| m.result) else { return false };
    let result = types.strip_implicit(result);
    types.u.method(result).is_some()
}
