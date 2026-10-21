// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Choosing among overloaded alternatives by their arguments (dotty's `resolveOverloaded`,
//! `compare`): those applicable without implicit conversions, else those applicable with.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::NodeId;
use tasty::{SymbolId, TypeId};

/// What the context expects of a tree (dotty's prototypes).
#[derive(Clone, Copy, Debug)]
pub enum Pt {
    Any,
    Type(TypeId),
    /// A function applied to the arguments of a `FunProto`, by its index in `Typer::protos`.
    Fun(usize),
    /// A polymorphic function applied to type arguments, by the index of its `PolyProto` in
    /// `Typer::poly_protos`.
    PolyFun(usize),
    /// The method of `m _`, to be eta-expanded (dotty's `AnyFunctionProto`).
    AnyFunction,
}

/// The arguments of an application, typed once when overloading needs their types, but for
/// functions left for the chosen alternative (dotty's `FunProto`).
pub struct FunProto {
    pub args: Vec<NodeId>,
    pub ctx: Ctx,
    pub typed: Option<Vec<Option<TNodeId>>>,
    /// What the application's result is expected to be (dotty's `resultType`).
    pub pt: Pt,
    /// Whether an expected result type takes part in the applicability of alternatives, not
    /// only ignored (dotty's `deepenProto` of an `IgnoredProto`).
    pub deep: bool,
    /// The elements of an infix operation's tuple argument, until it is untupled.
    pub dual: Option<Vec<NodeId>>,
    /// Whether the arguments are a `using` clause.
    pub using: bool,
}

impl Typer<'_> {
    /// dotty's `compare`: 1 if `a` is better, -1 if `b` is, 0 if neither; `None` if undecided.
    fn compare_alts(
        &mut self,
        (a, b): (SymbolId, SymbolId),
        prefix: Option<TypeId>,
        views: bool,
        ctx: Ctx,
    ) -> Option<i32> {
        let (oa, ob) = (self.u.symbol(a).owner?, self.u.symbol(b).owner?);
        let derives = |t: &mut Self, x, y| i32::from(x != y && t.u.derives_from(x, y));
        let owner_score = derives(self, oa, ob) - derives(self, ob, oa);
        let (pa, va) = self.rigid_param_types(a, prefix)?;
        let (pb, vb) = self.rigid_param_types(b, prefix)?;
        let wins_a = (!va || vb) && self.applicable(b, prefix, &pa, views, ctx)?;
        let wins_b = (!vb || va) && self.applicable(a, prefix, &pb, views, ctx)?;
        Some(match owner_score {
            1 => i32::from(wins_a || !wins_b),
            -1 => -i32::from(wins_b || !wins_a),
            _ => i32::from(wins_a && !wins_b) - i32::from(wins_b && !wins_a),
        })
    }

    /// The alternative of `alts`, members of `prefix`, that the arguments of `proto` select.
    pub fn resolve_overloaded(
        &mut self,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
        proto: usize,
    ) -> Result<SymbolId, &'static str> {
        let ctx = self.protos[proto].ctx;
        let mut applicable = Vec::new();
        let mut views = false;
        for phase in [false, true] {
            views = phase;
            let narrowed = self.narrow_by_shapes(alts, prefix, proto, views)?;
            if narrowed.len() < 2 {
                applicable = narrowed;
                if applicable.is_empty() {
                    continue;
                }
                break;
            }
            let arg_types = self.proto_arg_types(proto, &narrowed, prefix)?;
            for &alt in &narrowed {
                let fits = self.applicable_to_proto(alt, prefix, (proto, &arg_types), views, ctx);
                let fits = match (fits, self.protos[proto].pt) {
                    (Some(true), Pt::Type(pt)) if self.protos[proto].deep => {
                        Some(self.result_conforms(alt, prefix, pt, (views, ctx)) != Some(false))
                    }
                    _ => fits,
                };
                match fits {
                    Some(true) => applicable.push(alt),
                    Some(false) => {}
                    None => return Err("overload resolution with implicit conversions"),
                }
            }
            if !applicable.is_empty() {
                break;
            }
        }
        let Some((&first, rest)) = applicable.split_first() else { return Err("no applicable alternative") };
        let undecided = "overload resolution with implicit conversions";
        let mut best = first;
        for &alt in rest {
            if self.compare_alts((alt, best), prefix, views, ctx).ok_or(undecided)? > 0 {
                best = alt;
            }
        }
        let mut tied = vec![best];
        for &alt in applicable.iter().filter(|&&a| a != best) {
            if self.compare_alts((best, alt), prefix, views, ctx).ok_or(undecided)? <= 0 {
                tied.push(alt);
            }
        }
        if tied.len() == 1 {
            return Ok(self.adapted_by_result(best, alts, prefix, proto));
        }
        match self.protos[proto].pt {
            Pt::Fun(next) => return self.resolve_by_next_clause(&applicable, prefix, (proto, next)),
            Pt::PolyFun(_) => return Err("ambiguous overload resolved by further arguments"),
            _ => {}
        }
        for preferred in [false, true] {
            let kept: Vec<SymbolId> = alts
                .iter()
                .copied()
                .filter(|&a| if preferred { !self.has_defaults(a) } else { !self.result_is_method(a) })
                .collect();
            if let [only] = kept[..] {
                return Ok(only);
            }
            if kept.len() > 1 && kept.len() < alts.len() {
                return self.resolve_overloaded(&kept, prefix, proto);
            }
        }
        if matches!(self.protos[proto].pt, Pt::Type(_)) && !self.protos[proto].deep {
            self.protos[proto].deep = true;
            let deeper = self.resolve_overloaded(alts, prefix, proto);
            self.protos[proto].deep = false;
            return deeper;
        }
        self.ambiguous = applicable;
        Err("ambiguous overload")
    }

    /// The types of the arguments of `proto`, typed once for alternatives `alts` of `prefix`.
    pub fn proto_arg_types(
        &mut self,
        proto: usize,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
    ) -> Result<Vec<TypeId>, &'static str> {
        let args = self.pretyped_args(proto, alts, prefix)?;
        let mut arg_types = Vec::new();
        for (&a, &id) in args.iter().zip(&self.protos[proto].args.clone()) {
            arg_types.push(match a {
                Some(a) => {
                    let t = self.tpd.tpe(a);
                    self.zonk(t)
                }
                None => self.untyped_arg_type(id)?,
            });
        }
        Ok(arg_types)
    }

    /// Whether the first parameter clause of method `s` is followed by another that is not
    /// implicit (dotty's `resultIsMethod` with `stripInferrable`).
    fn result_is_method(&mut self, s: SymbolId) -> bool {
        let info = self.u.info(s);
        let info = match *self.u.types().get(info) {
            tasty::Type::Poly { result, .. } => result,
            _ => info,
        };
        let tasty::Type::Method { result, .. } = *self.u.types().get(info) else { return false };
        let result = self.strip_implicit(result);
        matches!(self.u.types().get(result), tasty::Type::Method { .. })
    }
}
