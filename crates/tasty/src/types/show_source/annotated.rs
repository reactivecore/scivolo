// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//   compiler/src/dotty/tools/dotc/util/Chars.scala
//! Annotated types and string constant types of `Printer.TypeReprCode`.

use super::{Highlight, REFUSED, Shown, SourcePrinter};
use crate::{TypeId, Universe};
use names::nme;

impl Universe {
    /// Whether the class of annotation type `annot` takes no arguments, so that the type shows
    /// the whole annotation.
    pub fn is_argless_annotation(&mut self, annot: TypeId) -> bool {
        let Some(class) = self.class_symbol(annot) else { return false };
        let ctors = self.decls_named(class, nme::CONSTRUCTOR);
        !ctors.is_empty() && ctors.into_iter().all(|c| self.erased_method(c).0.is_empty())
    }
}

impl SourcePrinter<'_> {
    /// `inner @annot`; refused for an annotation of a class that takes arguments, which the type
    /// does not keep.
    pub fn annotated(&mut self, inner: TypeId, annot: TypeId) -> Shown {
        if !self.u.is_argless_annotation(annot) {
            return Err(REFUSED);
        }
        self.print(inner)?;
        self.out.push_str(" @");
        self.print(annot)
    }

    /// String constant `s`, escaped (dotty's `Chars.escapedString`).
    pub fn string(&mut self, s: &str) {
        let mut text = String::from("\"");
        s.chars().for_each(|c| escape(&mut text, c));
        text.push('"');
        self.paint(Highlight::Literal, &text);
    }
}

/// Appends `c`, escaped as dotty's `Chars.escapedChar` does.
pub fn escape(s: &mut String, c: char) {
    let replace = match c {
        '\u{8}' => "\\b",
        '\t' => "\\t",
        '\n' => "\\n",
        '\u{c}' => "\\f",
        '\r' => "\\r",
        '"' => "\\\"",
        '\'' => "\\'",
        '\\' => "\\\\",
        c if c.is_control() => return s.push_str(&format!("\\u{:04x}", c as u32)),
        c => return s.push(c),
    };
    s.push_str(replace);
}
