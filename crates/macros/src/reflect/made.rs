// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions macros make: new value symbols, fresh names, `ValDef`s and copies of
//! definitions.

use super::{Answer, UNKNOWN, arg, numbers, symbol, text};
use crate::host::Host;

impl Host {
    /// Runs definition-making operation `op` on `args` and `name`.
    pub fn run_made(&mut self, op: &str, args: &[i32], name: &str) -> Answer {
        let arg = |i: usize| arg(args, i);
        let tree = |h: &Host, n: i32| match n {
            -1 => Ok(None),
            n => h.tree(n as u32).map(Some),
        };
        match op {
            "freshName" => {
                let fresh = self.fresh.next(&mut self.names, &format!("{name}$macro$"));
                text(self.names.text(fresh).to_owned())
            }
            "newVal" => {
                let (owner, info) = (symbol(arg(0)?)?, self.type_of(arg(1)? as u32)?);
                let flags = (arg(2)? as u32 as u64) | ((arg(3)? as u32 as u64) << 32);
                let s = self.reflect().new_val(owner, name, info, flags)?;
                numbers(vec![s.0 as i32])
            }
            "valDef" => {
                let (s, rhs) = (symbol(arg(0)?)?, tree(self, arg(1)?)?);
                let t = self.reflect().val_def(s, rhs)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "copyDef" => {
                let (original, tpt, rhs) =
                    (self.tree(arg(0)? as u32)?, self.tree(arg(1)? as u32)?, tree(self, arg(2)?)?);
                let params = args[3..].iter().map(|&p| self.tree(p as u32)).collect::<Result<Vec<_>, _>>()?;
                let t = self.reflect().copy_def(original, (name, &params), tpt, rhs)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }
}
