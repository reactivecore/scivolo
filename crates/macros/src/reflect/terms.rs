// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The terms of `quotes.reflect`.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;

impl Host {
    /// Runs term operation `op` on `args`.
    pub fn run_term(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        let arg = |i: usize| arg(args, i);
        match op {
            "underlying" => {
                let t = self.tree(arg(0)? as u32)?;
                let u = self.reflect().underlying(t);
                numbers(vec![self.add_tree(u) as i32])
            }
            "ref" => {
                let t = self.reflect().ref_(symbol(arg(0)?)?)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "select" => {
                let qual = self.tree(arg(0)? as u32)?;
                let t = self.reflect().select(qual, symbol(arg(1)?)?)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "appliedToTypes" => {
                let fun = self.tree(arg(0)? as u32)?;
                let types =
                    args[1..].iter().map(|&t| self.type_of(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let t = self.reflect().applied_to_types(fun, &types)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }
}
