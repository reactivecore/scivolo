// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! Quote patterns (dotty's `ExprMatch` and `TypeMatch`, `'[T] =?= '[P] && T <:< P` for types):
//! what the pattern's bindings and holes stand for, as the last match.

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::{Host, PATTERN};
use typer::inlining::{bound_types, quote_match};
use typer::unpickle::{PatternBody, QuotePattern};

impl Host {
    pub fn add_pattern(&mut self, p: QuotePattern) -> u32 {
        self.patterns.push(p);
        PATTERN | (self.patterns.len() as u32 - 1)
    }

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
                let PatternBody::Type(hole) = p.body else {
                    return Err("an expression pattern matched against a type");
                };
                (hole, p.bindings.clone())
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

    /// Matches expression `scrutinee` against quote pattern `pattern`: the number of things
    /// matched, `u32::MAX` if it does not match.
    pub fn match_expr(&mut self, scrutinee: u32, pattern: u32) -> Result<u32, &'static str> {
        let s = self.tree(scrutinee)?;
        let pattern = self.pattern(pattern).ok_or("a quote pattern the frontend did not unpickle")?;
        let Some((types, exprs)) = quote_match(&mut self.u, &mut self.tpd, s, &pattern)? else {
            return Ok(u32::MAX);
        };
        self.matched = types.into_iter().map(|t| Err(self.add_type(t))).collect();
        let exprs: Vec<_> = exprs.into_iter().map(|t| Ok(self.add_tree(t))).collect();
        self.matched.extend(exprs);
        Ok(self.matched.len() as u32)
    }
}
