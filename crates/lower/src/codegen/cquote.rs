// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//   util/src/main/scala/scala/scalanative/util/StringUtils.scala
//! C string literals `c"…"`: their bytes, escapes processed, as a constant (nscplugin's
//! `genCQuoteOp`, `StringUtils.processEscapes`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Op, Val, ValId};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use typer::tpd::{TNodeId, Tree};

pub const CQUOTE: &str = "scala.scalanative.unsafe.package$.CQuote";

/// The bytes of C string literal `s`, its escapes processed; `None` for an invalid escape.
fn process_escapes(s: &[u16]) -> Option<Vec<u8>> {
    let text = String::from_utf16(s).ok()?;
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' {
            let mut buf = [0; 4];
            out.extend_from_slice(chars[i].encode_utf8(&mut buf).as_bytes());
            i += 1;
            continue;
        }
        let c = *chars.get(i + 1)?;
        i += 2;
        let byte = match c {
            'b' => 8,
            't' => b'\t',
            'n' => b'\n',
            'f' => 12,
            'r' => b'\r',
            '"' | '\'' | '\\' => c as u8,
            '0'..='7' => return None,
            'x' => {
                let hex: String = chars[i..].iter().take_while(|c| c.is_ascii_hexdigit()).collect();
                i += hex.len();
                if chars.get(i).is_some_and(|c| c.is_numeric()) {
                    return None;
                }
                u8::try_from(u32::from_str_radix(&hex, 16).ok()?).ok()?
            }
            c if u32::from(c) > 0xFFFF => return None,
            c => {
                out.push(b'\\');
                u32::from(c) as u8
            }
        };
        out.push(byte);
    }
    Some(out)
}

impl Gen<'_, '_> {
    /// `CQuote(StringContext(s)).c()`, call `app` of `c` on `receiver`: a pointer to the bytes of
    /// `s`.
    pub fn gen_cquote(&mut self, receiver: TNodeId, span: Span) -> Result<ValId> {
        let refused = || unsupported(span, "a C string literal that is not one literal part");
        let arg = |g: &Self, t: TNodeId| match *g.l.tpd.tree(t) {
            Tree::Apply { args, .. } => match *g.l.tpd.items(args) {
                [a] => Some(a),
                _ => None,
            },
            _ => None,
        };
        let Some(seq) = arg(self, receiver).and_then(|c| arg(self, c)).and_then(|c| arg(self, c)) else {
            return refused();
        };
        let Tree::SeqLiteral { elems, .. } = *self.l.tpd.tree(self.l.tpd.strip_typed(seq)) else {
            return refused();
        };
        let &[part] = self.l.tpd.items(elems) else { return refused() };
        let Tree::Literal { value: Constant::String(ref s) } = *self.l.tpd.tree(part) else {
            return refused();
        };
        let Some(bytes) = process_escapes(s) else {
            return unsupported(span, "a C string literal with an invalid escape");
        };
        let bytes = self.val(Val::ByteString(bytes));
        let obj = self.val(Val::Const(bytes));
        let ty = self.class_type("scala.scalanative.unsafe.Ptr", false);
        Ok(self.emit(Op::Box { ty, obj }))
    }
}
