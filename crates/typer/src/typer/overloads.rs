//! Choosing among overloaded alternatives by their arguments (dotty's `resolveOverloaded`,
//! `isApplicableMethodRef`, `compare`), with the numeric widenings of the value classes as the
//! only implicit conversions; where another conversion could decide, outside the subset.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::NodeId;
use tasty::{SymbolId, Type, TypeId};

/// The arguments of an application, typed once without an expected type when overloading needs
/// their types (dotty's `FunProto`).
pub struct FunProto {
    pub args: Vec<NodeId>,
    pub ctx: Ctx,
    pub typed: Option<Vec<TNodeId>>,
}

impl Typer<'_> {
    /// Whether an argument of type `arg` fits a parameter of type `param` in `ctx` (dotty's
    /// `isCompatible`): `Some` if decided, `None` if an implicit conversion might decide it.
    fn compatible(&mut self, arg: TypeId, param: TypeId, ctx: Ctx) -> Option<bool> {
        if self.u.is_subtype(arg, param) {
            return Some(true);
        }
        if let (Some(a), Some(p)) = (self.value_class(arg), self.value_class(param))
            && super::widening::widens(&a, &p)
        {
            return Some(true);
        }
        (!self.view_may_exist(arg, param, ctx)).then_some(false)
    }

    /// Whether arguments of types `args` fit the first parameter clause of `alt` seen from
    /// `prefix` in `ctx`: `None` if undecided.
    fn applicable(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        args: &[TypeId],
        ctx: Ctx,
    ) -> Option<bool> {
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let info = self.method_info(tpe)?;
        let Type::Method { params, .. } = self.u.types().get(info).clone() else { return None };
        if params.len() != args.len() {
            return Some(false);
        }
        let mut all = true;
        for (&a, &(_, p)) in args.iter().zip(&params) {
            all &= self.compatible(a, p, ctx)?;
        }
        Some(all)
    }

    fn param_types(&mut self, alt: SymbolId, prefix: Option<TypeId>) -> Option<Vec<TypeId>> {
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let info = self.method_info(tpe)?;
        match self.u.types().get(info) {
            Type::Method { params, .. } => Some(params.iter().map(|&(_, p)| p).collect()),
            _ => None,
        }
    }

    /// dotty's `compare`: 1 if `a` is better, -1 if `b` is, 0 if neither; `None` if undecided.
    fn compare_alts(&mut self, a: SymbolId, b: SymbolId, prefix: Option<TypeId>, ctx: Ctx) -> Option<i32> {
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
        let pa = self.param_types(a, prefix)?;
        let pb = self.param_types(b, prefix)?;
        let wins_a = self.applicable(b, prefix, &pa, ctx)?;
        let wins_b = self.applicable(a, prefix, &pb, ctx)?;
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
        let arg_types: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let mut applicable = Vec::new();
        for &alt in alts {
            match self.applicable(alt, prefix, &arg_types, ctx) {
                Some(true) => applicable.push(alt),
                Some(false) => {}
                None => return Err("overload resolution with implicit conversions"),
            }
        }
        let Some((&first, rest)) = applicable.split_first() else { return Err("no applicable alternative") };
        let mut best = first;
        for &alt in rest {
            if self
                .compare_alts(alt, best, prefix, ctx)
                .ok_or("overload resolution with implicit conversions")?
                > 0
            {
                best = alt;
            }
        }
        for &alt in applicable.iter().filter(|&&a| a != best) {
            if self
                .compare_alts(best, alt, prefix, ctx)
                .ok_or("overload resolution with implicit conversions")?
                <= 0
            {
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
