// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The types of `quotes.reflect`.

use super::constants::encoded;
use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;
use tasty::{Type, TypeId};
use typer::tpd::constant_of;

impl Host {
    /// Runs type operation `op` on `args`.
    pub fn run_type(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let arg = |i: usize| arg(args, i);
        match op {
            "typeSymbol" => {
                let t = self.type_of(arg(0)? as u32)?;
                numbers(vec![self.reflect().type_symbol(t).map_or(-1, |s| s.0 as i32)])
            }
            "typeArgs" => {
                let t = self.type_of(arg(0)? as u32)?;
                let args = self.reflect().type_args(t);
                numbers(args.into_iter().map(|a| self.add_type(a) as i32).collect())
            }
            "widenTermRefByName" => {
                let t = self.type_of(arg(0)? as u32)?;
                let w = self.reflect().widen_term_ref_by_name(t);
                numbers(vec![self.add_type(w) as i32])
            }
            "constantType" => {
                let t = self.type_of(arg(0)? as u32)?;
                Ok(constant_of(&self.u, t).map_or((Vec::new(), None), encoded))
            }
            "typeConstructorOf" => {
                let t = self
                    .class_named(text)
                    .ok_or("TypeRepr.typeConstructorOf of a class the frontend lacks")?;
                numbers(vec![self.add_type(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }

    /// The type of the class of Java name `name`, a primitive type for a primitive's name; `None`
    /// for nested classes.
    fn class_named(&mut self, name: &str) -> Option<TypeId> {
        let primitive = ["boolean", "byte", "short", "char", "int", "long", "float", "double", "void"];
        let full = match primitive.iter().position(|&p| p == name) {
            Some(i) => format!(
                "scala.{}",
                ["Boolean", "Byte", "Short", "Char", "Int", "Long", "Float", "Double", "Unit"][i]
            ),
            None if name.contains('$') => return None,
            None => name.to_owned(),
        };
        let t = self.u.class_ref(&full);
        matches!(self.u.types().get(t), Type::Sym(..)).then_some(t)
    }
}
