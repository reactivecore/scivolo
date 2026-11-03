// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! The conditionals of null tests (dotty's `notNullConditional`, `CompareNull`).

use super::Typer;
use super::nullables::union;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use tasty::TypeId;

type Conditional = (Vec<TypeId>, Vec<TypeId>);

impl Typer<'_> {
    /// The references known not to be null if condition `t` is true and if it is false.
    pub fn not_null_conditional(&mut self, t: TNodeId) -> Conditional {
        let empty = (Vec::new(), Vec::new());
        if !self.explicit_nulls {
            return empty;
        }
        match self.tpd.tree(t).clone() {
            T::Block { expr, .. } | T::Inlined { expansion: expr, .. } => self.not_null_conditional(expr),
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
                let (T::Select { qualifier: x, .. }, [y]) =
                    (self.tpd.tree(fun).clone(), self.tpd.items(args))
                else {
                    return empty;
                };
                let y = *y;
                let op = self.boolean_op(fun);
                let ((xt, xf), (yt, yf)) = (self.not_null_conditional(x), self.not_null_conditional(y));
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
            (false, "eq" | "ne") => self.u.module_class(self.defn.predef) == Some(owner),
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

    pub fn is_null_literal(&self, t: TNodeId) -> bool {
        matches!(self.tpd.tree(t), T::Literal { value: Constant::Null })
    }

    /// The name of `Boolean` method `f` selects.
    pub fn boolean_op(&mut self, f: TNodeId) -> Option<String> {
        let m = self.tpd.sym(f)?;
        let owner = self.u.symbol(m).owner?;
        (self.u.full_name(owner) == "scala.Boolean").then(|| self.u.symbol(m).name.clone())
    }
}
