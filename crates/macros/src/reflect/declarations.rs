// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The declarations of classes by kind (`Symbol.declaredMethods`, `declaredFields`,
//! `declaredTypes` and their single forms) and type members.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use tasty::{Kind, SymbolId};

impl Host {
    /// Runs declaration operation `op` on `args` and `text`: what is declared (`methods`,
    /// `fields`, `types`, `all`) and the name looked for, empty for all.
    pub fn run_declaration(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let s = || symbol(arg(args, 0)?);
        match op {
            "declared" => {
                let (what, name) = text.split_once('\0').unwrap_or((text, ""));
                let s = s()?;
                let decls = if self.u.is_class(s) { self.u.symbol(s).decls.clone() } else { Vec::new() };
                let found = decls.into_iter().filter(|&d| {
                    self.is_declared(d, what) && (name.is_empty() || self.u.symbol(d).name == name)
                });
                numbers(found.map(|d| d.0 as i32).collect())
            }
            "typeMember" => {
                let s = s()?;
                let found = match self.u.is_class(s) {
                    true => self.u.member_where(s, text, |u, m| u.symbol(m).kind.is_type()),
                    false => Vec::new(),
                };
                numbers(vec![found.first().map_or(-1, |m| m.0 as i32)])
            }
            _ => Err(UNKNOWN),
        }
    }

    /// Whether declaration `d` is of kind `what`: a method other than a constructor, a field,
    /// a type, or any of them; never a local dummy.
    fn is_declared(&self, d: SymbolId, what: &str) -> bool {
        let s = self.u.symbol(d);
        if s.name.starts_with("<local ") {
            return false;
        }
        match what {
            "methods" => s.kind == Kind::Def && s.name != "<init>",
            "fields" => s.kind.is_term() && s.kind != Kind::Def,
            "types" => s.kind.is_type(),
            _ => true,
        }
    }
}
