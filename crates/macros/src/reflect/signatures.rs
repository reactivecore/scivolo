// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! `Symbol.signature`: per parameter clause the number of type parameters or the erased names of
//! the value parameters' types, and the erased name of the result type.

use crate::host::Host;
use tasty::{Erased, Kind, SymbolId, Type, TypeId};
use vm::Reflected;

impl Host {
    /// A number of type parameters per type clause and -1 per value parameter; the value
    /// parameters' names and then the result's, separated by `\0` (dotty's `NotAMethod`, no
    /// numbers and an empty text, for a symbol other than a method with parameters).
    pub fn signature(&mut self, s: SymbolId) -> Reflected {
        let mut t = self.u.info(s);
        let method = matches!(self.u.types().get(t), Type::Poly { .. } | Type::Method { .. });
        if self.u.symbol(s).kind != Kind::Def || !method {
            return (Vec::new(), Some(String::new()));
        }
        let (mut numbers, mut names) = (Vec::new(), Vec::new());
        loop {
            match self.u.types().get(t).clone() {
                Type::Poly { params, result, .. } => {
                    numbers.push(params.len() as i32);
                    t = result;
                }
                Type::Method { params, result, .. } => {
                    for (_, p) in params {
                        numbers.push(-1);
                        names.push(self.sig_name(p));
                    }
                    t = result;
                }
                _ => break,
            }
        }
        let result = match self.u.erase(t) {
            Erased::Unit => "scala.Unit".to_owned(),
            _ => self.sig_name(t),
        };
        names.push(result);
        (numbers, Some(names.join("\0")))
    }

    /// The name of the erasure of `t` in a signature (dotty's `sigName`): a class by its full
    /// name.
    fn sig_name(&mut self, t: TypeId) -> String {
        let erased = self.u.erase(t);
        self.erased_name(t, &erased)
    }

    fn erased_name(&mut self, t: TypeId, e: &Erased) -> String {
        match e {
            Erased::Class(name) | Erased::Value(name, _) => {
                let widened = self.u.widen(t);
                match self.u.class_symbol(widened) {
                    Some(c) if self.u.binary_name(c) == *name => self.u.full_name(c),
                    _ => name.clone(),
                }
            }
            Erased::Array(elem) => {
                let dealiased = self.u.dealias(t);
                let name = match self.u.types().get(dealiased).clone() {
                    Type::Applied(_, args) if args.len() == 1 => self.erased_name(args[0], elem),
                    _ => plain_name(elem),
                };
                format!("{name}[]")
            }
            e => plain_name(e),
        }
    }
}

/// The name of erased type `e` in a signature, a class by its binary name.
fn plain_name(e: &Erased) -> String {
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
        Erased::Array(elem) => format!("{}[]", plain_name(elem)),
    }
}
