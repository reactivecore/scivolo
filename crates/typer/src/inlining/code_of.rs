// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//   compiler/src/dotty/tools/dotc/printing/PlainPrinter.scala
//   compiler/src/dotty/tools/dotc/printing/RefinedPrinter.scala
//   compiler/src/dotty/tools/dotc/util/Chars.scala
//! `codeOf`: the argument tree as dotty's `RefinedPrinter` shows it, for literals, references
//! without a shown prefix, applications and infix operations of those.

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant as C;
use syntax::scan::is_operator_part;
use tasty::{Type, TypeId};

const REFUSED: &str = "codeOf an argument the frontend does not print";

impl Inlining<'_> {
    /// The text of argument `arg` as a string literal at `span` (dotty's `Intrinsics.codeOf`).
    pub fn code_of(&mut self, arg: TNodeId, span: Span) -> Result<TNodeId, Unsupported> {
        let inner = match *self.tpd.tree(arg) {
            T::Inlined { call: None, bindings, expansion } if bindings.is_empty() => expansion,
            _ => arg,
        };
        let text = self.code(inner, false).ok_or((span, REFUSED))?;
        Ok(self.literal(C::String(text.encode_utf16().collect()), span))
    }

    /// Tree `t` shown; `local` if it is an operand or a receiver, which prints only atoms.
    fn code(&mut self, t: TNodeId, local: bool) -> Option<String> {
        match self.tpd.tree(t).clone() {
            T::Literal { value } => constant_text(&value),
            T::Ident { name } => {
                let tpe = self.tpd.tpe(t);
                let prefix = match *self.u.types().get(tpe) {
                    Type::Sym(p, _) => p,
                    _ => return None,
                };
                prefix
                    .is_none_or(|p| self.is_omitted_prefix(p))
                    .then(|| self.names.text(name.name).to_owned())
            }
            T::Apply { .. } if local => None,
            T::Apply { fun, args } => {
                let args = self.tpd.items(args).to_vec();
                if let (T::Select { qualifier, name }, [arg]) = (self.tpd.tree(fun).clone(), &args[..]) {
                    let op = self.names.text(name.name).to_owned();
                    if op.chars().all(is_operator_part) {
                        let (l, r) = (self.code(qualifier, true)?, self.code(*arg, true)?);
                        return Some(format!("{l} {op} {r}"));
                    }
                }
                let fun = self.code(fun, true)?;
                let args: Option<Vec<String>> = args.iter().map(|&a| self.code(a, false)).collect();
                Some(format!("{fun}({})", args?.join(", ")))
            }
            _ => None,
        }
    }

    /// Whether `RefinedPrinter.toTextPrefixOf` leaves out prefix `p`: `scala`, `java.lang`,
    /// `Predef` and the empty package.
    fn is_omitted_prefix(&mut self, p: TypeId) -> bool {
        let s = match *self.u.types().get(p) {
            Type::Package(s) | Type::Sym(_, s) => s,
            Type::This(c) => match self.u.class_symbol(c) {
                Some(c) => c,
                None => return false,
            },
            _ => return false,
        };
        let full = self.u.full_name(s);
        matches!(full.as_str(), "scala" | "java.lang" | "scala.Predef" | "scala.Predef$" | "" | "<empty>")
    }
}

/// Constant `c` as dotty's `PlainPrinter.toText(Constant)` shows it, if it is one of the
/// integral, boolean, character or string kinds.
fn constant_text(c: &C) -> Option<String> {
    Some(match c {
        C::Boolean(b) => b.to_string(),
        C::Byte(n) => n.to_string(),
        C::Short(n) => n.to_string(),
        C::Int(n) => n.to_string(),
        C::Long(n) => format!("{n}L"),
        &C::Char(u) => format!("'{}'", escaped(char::from_u32(u as u32)?)),
        C::String(units) => {
            let chars: Option<String> =
                char::decode_utf16(units.iter().copied()).map(|c| c.ok().map(escaped)).collect();
            format!("\"{}\"", chars?)
        }
        _ => return None,
    })
}

/// `c` as dotty's `Chars.escapedChar` writes it.
fn escaped(c: char) -> String {
    match c {
        '\u{8}' => "\\b".into(),
        '\t' => "\\t".into(),
        '\n' => "\\n".into(),
        '\u{c}' => "\\f".into(),
        '\r' => "\\r".into(),
        '"' => "\\\"".into(),
        '\'' => "\\'".into(),
        '\\' => "\\\\".into(),
        c if c.is_control() => format!("\\u{:04x}", c as u32),
        c => c.into(),
    }
}
