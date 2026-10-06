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
        let args = self.pretyped_args(proto, alts, prefix);
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
    fn pretyped_args(&mut self, proto: usize, alts: &[SymbolId], prefix: Option<TypeId>) -> Vec<TNodeId> {
        if let Some(typed) = &self.protos[proto].typed {
            return typed.clone();
        }
        let (args, ctx) = (self.protos[proto].args.clone(), self.protos[proto].ctx);
        let mut typed = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let pt = self.common_function_formal(a, i, alts, prefix).map_or(Pt::Any, Pt::Type);
            typed.push(self.typed(a, pt, ctx));
        }
        self.protos[proto].typed = Some(typed.clone());
        typed
    }

    /// For function value `arg` at position `i` with parameters without types, the function type
    /// of the parameter types all of `alts` agree on there (dotty's `pretypeArgs`).
    fn common_function_formal(
        &mut self,
        arg: NodeId,
        i: usize,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
    ) -> Option<TypeId> {
        let syntax::ast::Tree::Function { args, .. } = *self.ast.tree(arg) else { return None };
        let params = self.ast.items(args).to_vec();
        let untyped = params.iter().any(|&p| {
            matches!(*self.ast.tree(p), syntax::ast::Tree::ValDef { tpt, .. } if matches!(self.ast.tree(tpt), syntax::ast::Tree::TypeTree))
        });
        if !untyped {
            return None;
        }
        let mut common: Option<Vec<TypeId>> = None;
        for &alt in alts {
            let (formals, _) = self.param_types(alt, prefix, false)?;
            let formal = *formals.get(i)?;
            let (ps, _) = (0..=22).find_map(|n| self.function_parts(formal, n))?;
            match &common {
                None => common = Some(ps),
                Some(c) if c.len() != ps.len() => return None,
                Some(c) => {
                    let same =
                        c.iter().zip(&ps).all(|(&a, &b)| self.u.is_subtype(a, b) && self.u.is_subtype(b, a));
                    if !same {
                        return None;
                    }
                }
            }
        }
        let mut fun_args = common?;
        if !fun_args.iter().all(|&a| self.is_fully_defined(a)) {
            return None;
        }
        let class = self.function_class(fun_args.len())?;
        fun_args.push(self.u.intern_type(tasty::Type::Wildcard(None)));
        Some(self.u.intern_type(tasty::Type::Applied(class, fun_args)))
    }
}
