// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SyntaxHighlight.scala
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala

use super::SourcePrinter;
use crate::SymbolId;

/// A colour of dotty's `SyntaxHighlight.ANSI`, or none.
#[derive(Clone, Copy)]
pub enum Highlight {
    Plain,
    Keyword,
    Type,
    Literal,
    Definition,
}

impl Highlight {
    /// Appends `s` to `out`, in this colour if `ansi`.
    pub fn write(self, out: &mut String, s: &str, ansi: bool) {
        let colour = match self {
            _ if !ansi => None,
            Highlight::Plain => None,
            Highlight::Keyword => Some("\u{1b}[33m"),
            Highlight::Definition => Some("\u{1b}[36m"),
            Highlight::Type | Highlight::Literal => Some("\u{1b}[32m"),
        };
        let Some(colour) = colour else {
            out.push_str(s);
            return;
        };
        out.push_str(colour);
        out.push_str(s);
        out.push_str("\u{1b}[0m");
    }
}

impl SourcePrinter<'_> {
    pub fn paint(&mut self, h: Highlight, s: &str) {
        h.write(&mut self.out, s, self.ansi);
    }

    /// Package `s`'s full name: each part in the type colour if `all` (a path of term
    /// references), else only the last (a package class's `this`).
    pub fn package_name(&mut self, s: SymbolId, all: bool) {
        let name = self.u.full_name(s);
        let parts: Vec<_> = name.split('.').collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                self.out.push('.');
            }
            match all || i + 1 == parts.len() {
                true => self.paint(Highlight::Type, part),
                false => self.out.push_str(part),
            }
        }
    }
}
