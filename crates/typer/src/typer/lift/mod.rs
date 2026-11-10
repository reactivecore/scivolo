// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala

mod app;
mod args;
mod value;

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{SymbolId, Type, TypeId};

/// Lifts the parts of one application into local values, evaluated once and in order: in
/// `f().x += 1` the qualifier `f()` becomes a local value whose `x` is read and then set, and a
/// call with named or default arguments evaluates them in the order written (dotty's `Lifter`:
/// `LiftComplex` if `complex`, else `LiftImpure`). Its methods
/// take the typer: deciding purity completes the methods trees refer to, and each lifted value
/// is a new local symbol.
pub struct Lifter {
    /// Whether only pure paths stay in place (`LiftComplex`), not every pure expression.
    complex: bool,
    ctx: Ctx,
    /// The definitions of the lifted values, in evaluation order.
    defs: Vec<TNodeId>,
}

impl Typer<'_> {
    /// Gives `expr`, or a reference to `val prefix$N = expr` added to `defs` if it needs lifting
    /// (dotty's `lift`); `$N$` for an empty prefix.
    pub fn lift(
        &mut self,
        defs: &mut Vec<TNodeId>,
        expr: TNodeId,
        prefix: &str,
        complex: bool,
        ctx: Ctx,
    ) -> TNodeId {
        let mut lifter = Lifter { complex, ctx, defs: Vec::new() };
        let lifted = lifter.lift(self, expr, prefix);
        defs.append(&mut lifter.defs);
        lifted
    }

    /// Gives `args`, of parameters `params`, each lifted unless it needs not or its parameter is
    /// by-name or inline, named after its parameter unless the name has a `$` (dotty's
    /// `liftArgs`); the definitions added to `defs` with their argument's index.
    pub fn lift_args(
        &mut self,
        defs: &mut Vec<(usize, TNodeId)>,
        params: &[(String, TypeId)],
        args: &[TNodeId],
        complex: bool,
        ctx: Ctx,
    ) -> Vec<TNodeId> {
        let mut lifter = Lifter { complex, ctx, defs: Vec::new() };
        let mut out = Vec::new();
        for (i, (&arg, param)) in args.iter().zip(params).enumerate() {
            out.push(lifter.arg(self, arg, param));
            defs.extend(lifter.defs.drain(..).map(|d| (i, d)));
        }
        out
    }

    /// Gives call `f` with its prefix and arguments lifted as far as evaluating them once and in
    /// order needs, the definitions added to `defs` (dotty's `liftApp`); `Err` for a function tree
    /// it does not lift.
    pub fn lift_app(
        &mut self,
        defs: &mut Vec<TNodeId>,
        f: TNodeId,
        complex: bool,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let mut lifter = Lifter { complex, ctx, defs: Vec::new() };
        let lifted = lifter.app(self, f);
        defs.append(&mut lifter.defs);
        lifted
    }

    /// Finds the `n` formal parameter types of lifted call `f`, by-name and repeated parameters as
    /// their underlying and element types, if its method type has as many parameters (dotty's
    /// `methType` from `liftedFunType`).
    pub fn lifted_formals(&mut self, f: TNodeId, n: usize) -> Option<Vec<TypeId>> {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Method { params, .. } = self.u.types().get(info).clone() else { return None };
        let last = params.len().checked_sub(1)?;
        let formals: Vec<TypeId> = (params.iter().enumerate())
            .map(|(i, &(_, p))| {
                let p = if i == last { self.repeated_param_elem(p).unwrap_or(p) } else { p };
                match *self.u.types().get(p) {
                    Type::ByName(r) => r,
                    _ => p,
                }
            })
            .collect();
        (formals.len() == n).then_some(formals)
    }

    /// Makes `to` the owner of the definitions in `tree` that `from` owns, as for an expression
    /// moved into the right-hand side of a new value (dotty's `changeNonLocalOwners`).
    pub fn change_owners(&mut self, tree: TNodeId, from: SymbolId, to: SymbolId) {
        let mut todo = vec![tree];
        while let Some(t) = todo.pop() {
            let definition = matches!(
                self.tpd.tree(t),
                T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. }
            );
            if definition
                && let Some(s) = self.tpd.sym(t)
                && self.u.symbol(s).owner == Some(from)
            {
                self.u.set_owner(s, to);
            }
            todo.extend(self.tpd.children(t));
        }
    }
}
