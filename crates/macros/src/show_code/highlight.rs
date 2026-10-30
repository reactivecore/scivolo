// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! The syntax highlighting of `Printer.TreeAnsiCode` and definition annotations.

use super::{Code, REFUSED, Shown};
use tasty::{Highlight, SymbolId};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    pub fn keyword(&mut self, s: &str) {
        Highlight::Keyword.write(&mut self.out, s, self.ansi);
    }

    pub fn highlight_type(&mut self, s: &str) {
        Highlight::Type.write(&mut self.out, s, self.ansi);
    }

    pub fn literal(&mut self, s: &str) {
        Highlight::Literal.write(&mut self.out, s, self.ansi);
    }

    pub fn definition_name(&mut self, s: &str) {
        Highlight::Definition.write(&mut self.out, s, self.ansi);
    }

    /// `(t) `, a condition.
    pub fn parens(&mut self, t: TNodeId) -> Shown {
        self.push("(");
        self.tree(t)?;
        self.push(") ");
        Ok(())
    }

    /// The annotations of definition `s`, each followed by a space (dotty's `printDefAnnotations`).
    pub fn def_annotations(&mut self, s: SymbolId) -> Shown {
        let annots = self.h.typer.annotations.get(&s).cloned().unwrap_or_default();
        for (i, &a) in annots.iter().enumerate() {
            if i > 0 {
                self.push(" ");
            }
            self.annotation(a)?;
        }
        if !annots.is_empty() {
            self.push(" ");
        }
        Ok(())
    }

    /// An annotation `new A(args)` as `@A(args)`.
    fn annotation(&mut self, a: TNodeId) -> Shown {
        let a = self.r(a);
        let (mut fun, mut args) = (a, Vec::new());
        loop {
            match self.tree_of(fun) {
                T::Apply { fun: f, args: a } => {
                    args.splice(0..0, self.items(a));
                    fun = self.r(f);
                }
                T::TypeApply { fun: f, .. } => fun = self.r(f),
                T::Select { qualifier, .. } => fun = self.r(qualifier),
                T::New { tpt } => {
                    self.push("@");
                    self.type_tree(tpt)?;
                    break;
                }
                _ => return Err(REFUSED),
            }
        }
        if !args.is_empty() {
            self.push("(");
            self.trees(&args, ", ")?;
            self.push(")");
        }
        Ok(())
    }
}
