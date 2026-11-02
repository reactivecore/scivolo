// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The not-null infos of typed trees and the conditionals of null tests (dotty's
//! `notNullInfo`, `notNullConditional`, `CompareNull`).

use super::Typer;
use super::nullables::NotNullInfo;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use tasty::TypeId;

type Conditional = (Vec<TypeId>, Vec<TypeId>);

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
        match *self.tpd.tree(t) {
            T::Inlined { expansion, .. } => self.not_null_info(expansion),
            T::Typed { expr, .. } => self.not_null_info(expr),
            T::TypeApply { fun, .. } => self.not_null_info(fun),
            T::Apply { fun, args } if self.is_throw(fun) => {
                let args = self.tpd.items(args).to_vec();
                args.iter().fold(NotNullInfo::empty(), |i, &a| i.seq(self.not_null_info(a))).terminated_info()
            }
            T::Apply { fun, args } => {
                let args = self.tpd.items(args).to_vec();
                let fun = self.not_null_info(fun);
                args.iter().fold(fun, |i, &a| i.seq(self.not_null_info(a)))
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
            _ => NotNullInfo::empty(),
        }
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

    /// The references known not to be null if condition `t` is true and if it is false.
    fn not_null_conditional(&mut self, t: TNodeId) -> Conditional {
        let empty = (Vec::new(), Vec::new());
        if !self.explicit_nulls {
            return empty;
        }
        match self.tpd.tree(t).clone() {
            T::Block { stats, expr } if self.tpd.items(stats).is_empty() => self.not_null_conditional(expr),
            T::Inlined { expansion, bindings, .. } if self.tpd.items(bindings).is_empty() => {
                self.not_null_conditional(expansion)
            }
            T::Select { qualifier, .. } if self.boolean_op(t).as_deref() == Some("unary_!") => {
                let (a, b) = self.not_null_conditional(qualifier);
                (b, a)
            }
            T::Apply { fun, args } => {
                if let Some((operand, equal)) = self.compare_null(fun, args) {
                    return match self.tracked(operand) {
                        Some(r) if equal => (Vec::new(), vec![r]),
                        Some(r) => (vec![r], Vec::new()),
                        None => empty,
                    };
                }
                let (T::Select { qualifier: x, .. }, [y]) = (self.tpd.tree(fun).clone(), self.tpd.items(args))
                else {
                    return empty;
                };
                let y = *y;
                let op = self.boolean_op(fun);
                let ((xt, xf), (yt, yf)) = (self.not_null_conditional(x), self.not_null_conditional(y));
                let union = |a: Vec<TypeId>, b: &[TypeId]| {
                    let mut a = a;
                    a.extend(b.iter().filter(|r| !a.contains(r)).copied().collect::<Vec<_>>());
                    a
                };
                let meet = |a: Vec<TypeId>, b: &[TypeId]| a.into_iter().filter(|r| b.contains(r)).collect();
                match op.as_deref() {
                    Some("&&") => (union(xt, &yt), meet(xf, &yf)),
                    Some("||") => (meet(xt, &yt), union(xf, &yf)),
                    _ => empty,
                }
            }
            _ => empty,
        }
    }

    /// The operand of null test `fun(args)` and whether it tests equality (dotty's `CompareNull`).
    fn compare_null(&mut self, fun: TNodeId, args: crate::tpd::List) -> Option<(TNodeId, bool)> {
        let &[arg] = self.tpd.items(args) else { return None };
        let (op, l) = match *self.tpd.tree(fun) {
            T::Select { qualifier, .. } => (fun, qualifier),
            T::Apply { fun: op, args: inner } => match *self.tpd.items(inner) {
                [l] => (op, l),
                _ => return None,
            },
            _ => return None,
        };
        let m = self.tpd.sym(op)?;
        let (name, owner) = (self.u.symbol(m).name.clone(), self.u.symbol(m).owner?);
        let b = self.u.builtins();
        let known = match (op == fun, name.as_str()) {
            (true, "==" | "!=") => owner == b.any,
            (true, "eq" | "ne") => owner == b.object || owner == self.defn.object,
            (false, "eq" | "ne") => self.u.full_name(owner).starts_with("scala.Predef"),
            _ => false,
        };
        let operand = match (self.is_null_literal(arg), self.is_null_literal(l)) {
            _ if !known => return None,
            (true, _) => l,
            (_, true) => arg,
            _ => return None,
        };
        Some((operand, matches!(name.as_str(), "==" | "eq")))
    }

    fn is_null_literal(&self, t: TNodeId) -> bool {
        matches!(self.tpd.tree(t), T::Literal { value: Constant::Null })
    }

    /// The name of `Boolean` method `f` selects.
    pub fn boolean_op(&mut self, f: TNodeId) -> Option<String> {
        let m = self.tpd.sym(f)?;
        let owner = self.u.symbol(m).owner?;
        (self.u.full_name(owner) == "scala.Boolean").then(|| self.u.symbol(m).name.clone())
    }

    fn is_throw(&self, fun: TNodeId) -> bool {
        self.tpd.sym(fun).is_some_and(|s| Some(s) == self.syms.throw_method)
    }
}
