// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SyntaxHighlight.scala
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! The syntax highlighting of `Printer.TreeAnsiCode` and definition annotations.

use super::{Code, REFUSED, Shown};
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree as T};

const RESET: &str = "\u{1b}[0m";
const KEYWORD: &str = "\u{1b}[33m";
const DEFINITION: &str = "\u{1b}[36m";
const LITERAL: &str = "\u{1b}[32m";
const TYPE: &str = "\u{1b}[32m";

impl Code<'_> {
    fn highlight(&mut self, color: &str, s: &str) {
        match self.ansi {
            true => self.out.push_str(&format!("{color}{s}{RESET}")),
            false => self.out.push_str(s),
        }
    }

    pub fn keyword(&mut self, s: &str) {
        self.highlight(KEYWORD, s);
    }

    pub fn highlight_type(&mut self, s: &str) {
        self.highlight(TYPE, s);
    }

    pub fn literal(&mut self, s: &str) {
        self.highlight(LITERAL, s);
    }

    pub fn definition_name(&mut self, s: &str) {
        self.highlight(DEFINITION, s);
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
