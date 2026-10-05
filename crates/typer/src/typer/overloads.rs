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
    /// A polymorphic function applied to type arguments (dotty's `PolyProto`).
    PolyFun,
}

/// The arguments of an application, typed once without an expected type when overloading needs
/// their types (dotty's `FunProto`).
pub struct FunProto {
    pub args: Vec<NodeId>,
    pub ctx: Ctx,
    pub typed: Option<Vec<TNodeId>>,
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
        let owner_score = if oa == ob {
            0
        } else if self.u.derives_from(oa, ob) {
            1
        } else if self.u.derives_from(ob, oa) {
            -1
        } else {
            0
        };
        let (pa, va) = self.param_types(a, prefix, false)?;
        let (pb, vb) = self.param_types(b, prefix, false)?;
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
        let args = self.pretyped_args(proto);
        let ctx = self.protos[proto].ctx;
        let mut arg_types = Vec::new();
        for &a in &args {
            let t = self.tpd.tpe(a);
            arg_types.push(self.zonk(t));
        }
        let mut applicable = Vec::new();
        let mut views = false;
        for phase in [false, true] {
            views = phase;
            for &alt in alts {
                match self.applicable(alt, prefix, &arg_types, views, ctx) {
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
        for &alt in applicable.iter().filter(|&&a| a != best) {
            if self.compare_alts((best, alt), prefix, views, ctx).ok_or(undecided)? <= 0 {
                return Err("ambiguous overload");
            }
        }
        Ok(best)
    }

    /// The arguments of `proto`, typed without an expected type once.
    fn pretyped_args(&mut self, proto: usize) -> Vec<TNodeId> {
        if let Some(typed) = &self.protos[proto].typed {
            return typed.clone();
        }
        let (args, ctx) = (self.protos[proto].args.clone(), self.protos[proto].ctx);
        let typed: Vec<TNodeId> = args.into_iter().map(|a| self.typed(a, Pt::Any, ctx)).collect();
        self.protos[proto].typed = Some(typed.clone());
        typed
    }
}
