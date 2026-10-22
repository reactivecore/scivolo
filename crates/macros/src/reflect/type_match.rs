// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! Quoted type patterns `'[P]` (dotty's `TypeMatch`, `'[T] =?= '[P] && T <:< P`): the types
//! the pattern's bindings stand for, as the last match.

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::{Host, PATTERN};
use typer::inlining::bound_types;

impl Host {
    /// Runs type pattern operation `op` on `args`.
    pub fn run_type_match(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        if op != "matchType" {
            return Err(UNKNOWN);
        }
        let scrutinee = self.type_of(arg(args, 0)? as u32)?;
        let pattern = arg(args, 1)? as u32;
        let (hole, binds) = match pattern & PATTERN != 0 {
            true => {
                let p = self.patterns.get((pattern & !PATTERN) as usize);
                let p = p.ok_or("a type pattern the frontend did not unpickle")?;
                (p.hole, p.bindings.clone())
            }
            false => (self.type_of(pattern)?, Vec::new()),
        };
        let Some(types) = bound_types(&mut self.u, scrutinee, hole, &binds) else {
            self.decided(false, [scrutinee, hole])?;
            return numbers(vec![-1]);
        };
        let types: Vec<_> = types.into_iter().map(|t| Err(self.add_type(t))).collect();
        self.matched = types;
        numbers(vec![self.matched.len() as i32])
    }
}
