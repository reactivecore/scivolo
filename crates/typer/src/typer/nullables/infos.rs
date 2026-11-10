// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The not-null infos of typed trees (dotty's `notNullInfo` and the infos `Typer` attaches).

use super::super::Typer;
use super::NotNullInfo;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Typer<'_> {
    /// The references known not to be null after `t` completes normally.
    pub fn not_null_info(&mut self, t: TNodeId) -> NotNullInfo {
        if !self.explicit_nulls {
            return NotNullInfo::empty();
        }
        if let Some(i) = self.nullables.infos.get(&t) {
            return i.clone();
        }
        let info = self.collect_not_null_info(t);
        self.nullables.infos.insert(t, info.clone());
        info
    }

    fn collect_not_null_info(&mut self, t: TNodeId) -> NotNullInfo {
        match self.tpd.tree(t).clone() {
            T::Inlined { expansion, .. } => self.not_null_info(expansion),
            T::Typed { expr, .. } => self.not_null_info(expr),
            T::TypeApply { fun, .. } => self.not_null_info(fun),
            T::Apply { fun, args } => {
                let args = self.tpd.items(args).to_vec();
                let fun = self.not_null_info(fun);
                let info = args.iter().fold(fun, |i, &a| i.seq(self.not_null_info(a)));
                if self.is_nothing_type(t) { info.terminated_info() } else { info }
            }
            T::Block { stats, expr } => {
                let stats = self.tpd.items(stats).to_vec();
                let expr = self.not_null_info(expr);
                stats.iter().rev().fold(expr, |i, &s| self.not_null_info(s).seq(i))
            }
            T::ValDef { rhs: Some(rhs), .. } => {
                let info = self.not_null_info(rhs);
                let lazy = self.tpd.sym(t).is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::LAZY));
                if lazy { info.retracted_info() } else { info }
            }
            T::If { cond, thenp, elsep, .. } => {
                let then_path = self.not_null_info_if(cond, true).seq(self.not_null_info(thenp));
                let else_path = self.not_null_info_if(cond, false).seq(self.not_null_info(elsep));
                then_path.alt(else_path)
            }
            T::WhileDo { cond, body } => {
                self.not_null_info(body).retracted_info().seq(self.not_null_info_if(cond, false))
            }
            T::Return { expr, .. } => self.not_null_info(expr).terminated_info(),
            T::Assign { lhs, rhs } => self.assign_info(lhs, rhs),
            T::Match { selector, cases, .. } => {
                let cases = self.tpd.items(cases).to_vec();
                let infos: Vec<NotNullInfo> = cases.iter().map(|&c| self.not_null_info(c)).collect();
                match infos.into_iter().reduce(NotNullInfo::alt) {
                    Some(info) => info,
                    None => selector.map_or_else(NotNullInfo::empty, |s| self.not_null_info(s)),
                }
            }
            T::CaseDef { pat, guard, body } => {
                let guard = guard.map_or_else(NotNullInfo::empty, |g| self.not_null_info_if(g, true));
                self.not_null_info(pat).seq(guard).seq(self.not_null_info(body))
            }
            T::Try { expr, cases, finalizer } => {
                let cases = self.tpd.items(cases).to_vec();
                let after_cases = self.after_cases_info(expr, &cases);
                let finalizer = finalizer.map_or_else(NotNullInfo::empty, |f| self.not_null_info(f));
                self.not_null_info(expr).alt(after_cases).seq(finalizer)
            }
            _ => NotNullInfo::empty(),
        }
    }

    /// The info after `try` expression `expr` failed and one of handler `cases` completed.
    fn after_cases_info(&mut self, expr: TNodeId, cases: &[TNodeId]) -> NotNullInfo {
        let terminated = NotNullInfo::empty().terminated_info();
        let cases = cases.iter().fold(terminated, |i, &c| i.alt(self.not_null_info(c)));
        self.not_null_info(expr).retracted_info().seq(cases)
    }

    /// `ctx` of the finalizer of `try` expression `expr` with handler `cases`.
    pub fn finalizer_ctx(&mut self, expr: TNodeId, cases: &[TNodeId], ctx: Ctx) -> Ctx {
        let info = self.after_cases_info(expr, cases).retracted_info();
        self.with_not_null(ctx, info)
    }

    /// The info of `lhs = rhs` (dotty's `computeAssignNullable`).
    fn assign_info(&mut self, lhs: TNodeId, rhs: TNodeId) -> NotNullInfo {
        let info = self.not_null_info(rhs);
        let Some(r) = self.tracked(lhs).filter(|&r| self.strip_null(r) != r) else { return info };
        let rhs = self.tpd.tpe(rhs);
        let nullable = self.types().is_null_type(rhs) || self.strip_null(rhs) != rhs;
        info.seq(if nullable { NotNullInfo::retracting(vec![r]) } else { NotNullInfo::of(vec![r]) })
    }

    /// The info of condition `t` when it is `c` (dotty's `notNullInfoIf`).
    pub fn not_null_info_if(&mut self, t: TNodeId, c: bool) -> NotNullInfo {
        let (if_true, if_false) = self.not_null_conditional(t);
        let info = self.not_null_info(t);
        if if_true.is_empty() && if_false.is_empty() {
            return info;
        }
        info.seq(NotNullInfo::of(if c { if_true } else { if_false }))
    }

    /// Whether the type of `t` is a reference to class `Nothing` (dotty's `isNothingType`).
    fn is_nothing_type(&mut self, t: TNodeId) -> bool {
        let (nothing, tpe) = (self.u.builtins().nothing, self.tpd.tpe(t));
        match *self.u.types().get(tpe) {
            Type::Sym(_, s) => s == nothing,
            Type::TypeRef(..) => self.u.resolve_ref(tpe) == Some(nothing),
            _ => false,
        }
    }
}
