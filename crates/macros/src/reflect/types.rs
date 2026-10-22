// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The types of `quotes.reflect`.

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;

impl Host {
    /// Runs type operation `op` on `args`.
    pub fn run_type(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
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
            _ => Err(UNKNOWN),
        }
    }
}
