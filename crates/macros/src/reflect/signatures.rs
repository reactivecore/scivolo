// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! `Symbol.signature`: per parameter clause the number of type parameters or the erased names of
//! the value parameters' types, and the erased name of the result type.

use crate::host::Host;
use tasty::{Erased, Kind, SymbolId, Type};
use vm::Reflected;

impl Host {
    /// A number of type parameters per type clause and -1 per value parameter; the value
    /// parameters' names and then the result's, separated by `\0` (dotty's `NotAMethod`, no
    /// numbers and an empty text, for a symbol other than a method).
    pub fn signature(&mut self, s: SymbolId) -> Reflected {
        if self.u.symbol(s).kind != Kind::Def {
            return (Vec::new(), Some(String::new()));
        }
        let (mut numbers, mut names) = (Vec::new(), Vec::new());
        let mut t = self.u.info(s);
        loop {
            match self.u.types().get(t).clone() {
                Type::Poly { params, result, .. } => {
                    numbers.push(params.len() as i32);
                    t = result;
                }
                Type::Method { params, result, .. } => {
                    for (_, p) in params {
                        numbers.push(-1);
                        let erased = self.u.erase(p);
                        names.push(sig_name(&erased));
                    }
                    t = result;
                }
                _ => break,
            }
        }
        let result = self.u.erase(t);
        names.push(sig_name(&result));
        (numbers, Some(names.join("\0")))
    }
}

/// The name of erased type `e` in a signature (dotty's `sigName`).
fn sig_name(e: &Erased) -> String {
    match e {
        Erased::Prim(c) => {
            let names = [('z', "Boolean"), ('c', "Char"), ('b', "Byte"), ('s', "Short"), ('i', "Int")];
            let more = [('j', "Long"), ('f', "Float"), ('d', "Double")];
            let name = names.iter().chain(&more).find(|(p, _)| p == c).map_or("Any", |(_, n)| n);
            format!("scala.{name}")
        }
        Erased::Unit => "scala.runtime.BoxedUnit".to_owned(),
        Erased::Nothing => "scala.Nothing".to_owned(),
        Erased::Null => "scala.Null".to_owned(),
        Erased::Class(name) | Erased::Value(name, _) => name.clone(),
        Erased::Array(elem) => format!("{}[]", sig_name(elem)),
    }
}
