// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The errors of `quotes.reflect.report`, recorded with their spans.

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;

impl Host {
    /// Runs report operation `op` on `args` and message `text`.
    pub fn run_report(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        match op {
            "error" => {
                let tree = arg(args, 0)?;
                let span = match tree {
                    t if t >= 0 => self.tpd.span(self.tree(t as u32)?).without_inlined_call(),
                    _ => self.expansion,
                };
                self.reported.push((span, text.to_owned()));
                numbers(Vec::new())
            }
            "errorAt" => {
                let span = self.span_of((arg(args, 0)?, arg(args, 1)?, arg(args, 2)?))?;
                self.reported.push((span, text.to_owned()));
                numbers(Vec::new())
            }
            _ => Err(UNKNOWN),
        }
    }
}
