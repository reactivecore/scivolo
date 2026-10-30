// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Refinements and match types as `SourceCode.printType` shows them, in indented blocks.

use super::{Shown, SourcePrinter};
use crate::types::{Type, TypeId};

impl SourcePrinter<'_> {
    fn line_break(&mut self) {
        self.out.push('\n');
        self.out.push_str(&"  ".repeat(self.indent));
    }

    /// `scrutinee match {` and the cases on their own lines.
    pub fn match_type(&mut self, scrutinee: TypeId, cases: &[TypeId]) -> Shown {
        self.print(scrutinee)?;
        self.out.push_str(" match  {");
        self.indent += 1;
        for &c in cases {
            self.line_break();
            self.print(c)?;
        }
        self.indent -= 1;
        self.line_break();
        self.out.push('}');
        Ok(())
    }

    /// Refinement `t` (dotty's `printRefinement`): its parent, then its members in a block.
    pub fn refinement(&mut self, t: TypeId) -> Shown {
        self.refinements(t)?;
        self.line_break();
        self.out.push('}');
        Ok(())
    }

    fn refinements(&mut self, t: TypeId) -> Shown {
        let Type::Refined(parent, name, info) = self.u.types().get(t).clone() else {
            self.print(t)?;
            self.out.push_str(" {");
            return Ok(());
        };
        self.refinements(parent)?;
        self.indent += 1;
        self.line_break();
        let bounds = match *self.u.types().get(info) {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            Type::Alias(a) => Some((a, a)),
            _ => None,
        };
        let def =
            matches!(self.u.types().get(info), Type::ByName(_) | Type::Method { .. } | Type::Lambda { .. });
        let shown = match bounds {
            Some((lo, hi)) => {
                self.out.push_str(&format!("type {name}"));
                self.bounds(lo, hi)
            }
            None => {
                self.out.push_str(&format!("{} {name}", if def { "def" } else { "val" }));
                self.methodic(info)
            }
        };
        self.indent -= 1;
        shown
    }

    /// The type of a refined method or value (dotty's `printMethodicType`).
    fn methodic(&mut self, t: TypeId) -> Shown {
        match self.u.types().get(t).clone() {
            Type::Method { binder, params, result, .. } => self.bind(binder, &params, |p| {
                p.out.push('(');
                p.params(&params)?;
                p.out.push(')');
                p.methodic(result)
            }),
            Type::Lambda { binder, params, result } => self.bind(binder, &params, |p| {
                p.out.push('[');
                p.params(&params)?;
                p.out.push(']');
                p.methodic(result)
            }),
            Type::ByName(r) => {
                self.out.push_str(": ");
                self.print(r)
            }
            _ => {
                self.out.push_str(": ");
                self.print(t)
            }
        }
    }
}
