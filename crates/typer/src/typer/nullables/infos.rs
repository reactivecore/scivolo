// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The not-null infos of typed trees (dotty's `notNullInfo` and the infos `Typer` attaches).

use super::super::Typer;
use super::{Flow, NotNullInfo, Nullables, flow};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Typer<'_> {
    /// Finds the references known not to be null after `t` completes normally: in
    /// `if x == null then return; x.length` those after the `if`, `x`.
    pub fn not_null_info(&mut self, t: TNodeId) -> NotNullInfo {
        if !self.explicit_nulls {
            return NotNullInfo::empty();
        }
        let (nullables, mut f) = flow(self);
        nullables.not_null_info(&mut f, t)
    }

    /// Gives `ctx` what is known after tree `t` completed normally (dotty's `nullableContext`).
    pub fn nullable_ctx(&mut self, t: TNodeId, ctx: Ctx) -> Ctx {
        let info = self.not_null_info(t);
        self.nullables.with_not_null(ctx, info)
    }

    /// Gives `ctx` what is known when condition `t` is `c`, as `x != null` in the `then` branch of
    /// `if x != null then x.length else 0` (dotty's `nullableContextIf`).
    pub fn nullable_ctx_if(&mut self, t: TNodeId, c: bool, ctx: Ctx) -> Ctx {
        if !self.explicit_nulls {
            return ctx;
        }
        let (nullables, mut f) = flow(self);
        let info = nullables.not_null_info_if(&mut f, t, c);
        nullables.with_not_null(ctx, info)
    }

    /// Gives `ctx` of the finalizer of `try` expression `expr` with handler `cases` what is known
    /// whether `expr` or a handler completed.
    pub fn finalizer_ctx(&mut self, expr: TNodeId, cases: &[TNodeId], ctx: Ctx) -> Ctx {
        if !self.explicit_nulls {
            return ctx;
        }
        let (nullables, mut f) = flow(self);
        let info = nullables.after_cases_info(&mut f, expr, cases).retracted_info();
        nullables.with_not_null(ctx, info)
    }
}

impl Nullables {
    /// Finds the references known not to be null after `t` completes normally, remembered.
    fn not_null_info(&mut self, f: &mut Flow, t: TNodeId) -> NotNullInfo {
        if let Some(i) = self.infos.get(&t) {
            return i.clone();
        }
        let info = self.collect_not_null_info(f, t);
        self.infos.insert(t, info.clone());
        info
    }

    fn collect_not_null_info(&mut self, f: &mut Flow, t: TNodeId) -> NotNullInfo {
        match f.tpd.tree(t).clone() {
            T::Inlined { expansion, .. } => self.not_null_info(f, expansion),
            T::Typed { expr, .. } => self.not_null_info(f, expr),
            T::TypeApply { fun, .. } => self.not_null_info(f, fun),
            T::Apply { fun, args } => {
                let args = f.tpd.items(args).to_vec();
                let fun = self.not_null_info(f, fun);
                let info = args.iter().fold(fun, |i, &a| i.seq(self.not_null_info(f, a)));
                if is_nothing_type(f, t) { info.terminated_info() } else { info }
            }
            T::Block { stats, expr } => {
                let stats = f.tpd.items(stats).to_vec();
                let expr = self.not_null_info(f, expr);
                stats.iter().rev().fold(expr, |i, &s| self.not_null_info(f, s).seq(i))
            }
            T::ValDef { rhs: Some(rhs), .. } => {
                let info = self.not_null_info(f, rhs);
                let lazy = f.tpd.sym(t).is_some_and(|s| f.types.u.symbol(s).flags.has(tasty::tags::LAZY));
                if lazy { info.retracted_info() } else { info }
            }
            T::If { cond, thenp, elsep, .. } => {
                let then_path = self.not_null_info_if(f, cond, true).seq(self.not_null_info(f, thenp));
                let else_path = self.not_null_info_if(f, cond, false).seq(self.not_null_info(f, elsep));
                then_path.alt(else_path)
            }
            T::WhileDo { cond, body } => {
                self.not_null_info(f, body).retracted_info().seq(self.not_null_info_if(f, cond, false))
            }
            T::Return { expr, .. } => self.not_null_info(f, expr).terminated_info(),
            T::Assign { lhs, rhs } => self.assign_info(f, lhs, rhs),
            T::Match { selector, cases, .. } => {
                let cases = f.tpd.items(cases).to_vec();
                let infos: Vec<NotNullInfo> = cases.iter().map(|&c| self.not_null_info(f, c)).collect();
                match infos.into_iter().reduce(NotNullInfo::alt) {
                    Some(info) => info,
                    None => selector.map_or_else(NotNullInfo::empty, |s| self.not_null_info(f, s)),
                }
            }
            T::CaseDef { pat, guard, body } => {
                let guard = guard.map_or_else(NotNullInfo::empty, |g| self.not_null_info_if(f, g, true));
                self.not_null_info(f, pat).seq(guard).seq(self.not_null_info(f, body))
            }
            T::Try { expr, cases, finalizer } => {
                let cases = f.tpd.items(cases).to_vec();
                let after_cases = self.after_cases_info(f, expr, &cases);
                let finalizer = finalizer.map_or_else(NotNullInfo::empty, |x| self.not_null_info(f, x));
                self.not_null_info(f, expr).alt(after_cases).seq(finalizer)
            }
            _ => NotNullInfo::empty(),
        }
    }

    /// Finds the info after `try` expression `expr` failed and one of handler `cases` completed.
    fn after_cases_info(&mut self, f: &mut Flow, expr: TNodeId, cases: &[TNodeId]) -> NotNullInfo {
        let terminated = NotNullInfo::empty().terminated_info();
        let cases = cases.iter().fold(terminated, |i, &c| i.alt(self.not_null_info(f, c)));
        self.not_null_info(f, expr).retracted_info().seq(cases)
    }

    /// Finds the info of `lhs = rhs` (dotty's `computeAssignNullable`).
    fn assign_info(&mut self, f: &mut Flow, lhs: TNodeId, rhs: TNodeId) -> NotNullInfo {
        let info = self.not_null_info(f, rhs);
        let Some(r) = self.tracked(f, lhs).filter(|&r| f.types.strip_null(r) != r) else { return info };
        let rhs = f.tpd.tpe(rhs);
        let nullable = f.types.is_null_type(rhs) || f.types.strip_null(rhs) != rhs;
        info.seq(if nullable { NotNullInfo::retracting(vec![r]) } else { NotNullInfo::of(vec![r]) })
    }

    /// Finds the info of condition `t` when it is `c` (dotty's `notNullInfoIf`).
    fn not_null_info_if(&mut self, f: &mut Flow, t: TNodeId, c: bool) -> NotNullInfo {
        let conditional = self.not_null_conditional(f, t);
        let info = self.not_null_info(f, t);
        if conditional.if_true.is_empty() && conditional.if_false.is_empty() {
            return info;
        }
        info.seq(NotNullInfo::of(if c { conditional.if_true } else { conditional.if_false }))
    }
}

/// Whether the type of `t` is a reference to class `Nothing` (dotty's `isNothingType`).
fn is_nothing_type(f: &mut Flow, t: TNodeId) -> bool {
    let u = &mut *f.types.u;
    let (nothing, tpe) = (u.defn().nothing, f.tpd.tpe(t));
    match *u.ty(tpe) {
        Type::Sym(_, s) => s == nothing,
        Type::TypeRef(..) => u.resolve_ref(tpe) == Some(nothing),
        _ => false,
    }
}
