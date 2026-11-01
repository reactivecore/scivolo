// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Control expressions of `Printer.TreeCode`: `if`, `match`, `try`, `return` and `while`.

use super::{Code, REFUSED, Shown};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    /// Control expression `t`.
    pub fn control(&mut self, t: TNodeId) -> Shown {
        match self.tree_of(t) {
            T::If { cond, thenp, elsep, .. } => {
                self.keyword("if ");
                self.parens(cond)?;
                self.tree(thenp)?;
                self.keyword(" else ");
                self.tree(elsep)
            }
            T::Match { selector, cases, .. } => {
                match selector {
                    Some(s) => {
                        self.qual_tree(s)?;
                        self.keyword(" match");
                    }
                    None => self.keyword("summonFrom "),
                }
                self.cases(&self.items(cases))
            }
            T::Try { expr, cases, finalizer } => {
                self.keyword("try ");
                self.tree(expr)?;
                let cases = self.items(cases);
                if !cases.is_empty() {
                    self.keyword(" catch");
                    self.cases(&cases)?;
                }
                if let Some(f) = finalizer {
                    self.keyword(" finally ");
                    self.tree(f)?;
                }
                Ok(())
            }
            T::Return { expr, .. } => {
                self.push("return ");
                self.tree(expr)
            }
            T::WhileDo { cond, body } => {
                self.keyword("while ");
                self.parens(cond)?;
                self.tree(body)
            }
            _ => Err(REFUSED),
        }
    }

    /// A qualifier, in parentheses if it is a control expression.
    pub fn qual_tree(&mut self, t: TNodeId) -> Shown {
        match self.tree_of(t) {
            T::If { .. } | T::Match { .. } | T::WhileDo { .. } | T::Try { .. } | T::Return { .. } => {
                self.push("(");
                self.tree(t)?;
                self.push(")");
                Ok(())
            }
            _ => self.tree(t),
        }
    }
}
