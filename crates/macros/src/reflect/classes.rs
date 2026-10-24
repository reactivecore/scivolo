// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The classes macros make: `Symbol.newClass`, `ClassDef.apply`, `TypeIdent` and `New`.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;

impl Host {
    /// Runs class-making operation `op` on `args` and `name`.
    pub fn run_class(&mut self, op: &str, args: &[i32], name: &str) -> Answer {
        let rest = |from: usize| args.get(from..).unwrap_or_default();
        match op {
            "newClass" => {
                let owner = match symbol(arg(args, 0)?)? {
                    o if o == self.macro_owner() => self.owner,
                    o => o,
                };
                let parents =
                    rest(1).iter().map(|&p| self.type_of(p as u32)).collect::<Result<Vec<_>, _>>()?;
                let c = self.reflect().new_class(owner, name, parents)?;
                numbers(vec![c.0 as i32])
            }
            "enterDecls" => {
                let decls = rest(1).iter().map(|&d| symbol(d)).collect::<Result<Vec<_>, _>>()?;
                self.reflect().enter_decls(symbol(arg(args, 0)?)?, &decls)?;
                numbers(Vec::new())
            }
            "classDef" => {
                let (cls, n) = (symbol(arg(args, 0)?)?, arg(args, 1)? as usize);
                let trees = rest(2).iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let (parents, body) =
                    trees.split_at_checked(n).ok_or("ClassDef.apply without its parents")?;
                let t = self.reflect().class_def(cls, parents, body)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "typeIdent" => {
                let t = self.reflect().type_ident(symbol(arg(args, 0)?)?)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "new" => {
                let tpt = self.tree(arg(args, 0)? as u32)?;
                let t = self.reflect().new_(tpt);
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }
}
