// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The terms of `quotes.reflect`.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;

impl Host {
    /// Runs term operation `op` on `args`.
    pub fn run_term(&mut self, op: &str, args: &[i32], name: &str) -> Answer {
        let arg = |i: usize| arg(args, i);
        match op {
            "underlying" => {
                let t = self.tree(arg(0)? as u32)?;
                let u = self.reflect().underlying(t);
                numbers(vec![self.add_tree(u) as i32])
            }
            "underlyingArgument" => {
                let t = self.tree(arg(0)? as u32)?;
                let u = self.reflect().underlying_argument(t);
                numbers(vec![self.add_tree(u) as i32])
            }
            "tpe" => {
                let t = self.tree(arg(0)? as u32)?;
                let tpe = self.tpd.get(t).tpe.ok_or("the type of a tree without one")?;
                numbers(vec![self.add_type(tpe) as i32])
            }
            "isExprOf" => {
                let t = self.tree(arg(0)? as u32)?;
                let tpe = self.tpd.get(t).tpe.ok_or("isExprOf of a tree without a type")?;
                let of = self.type_of(arg(1)? as u32)?;
                let holds = self.u.is_subtype_in(tpe, of, &mut tasty::NoVars);
                self.decided(holds, [tpe, of])
            }
            "typeTree" => {
                let tpe = self.type_of(arg(0)? as u32)?;
                let t = self.reflect().type_tree(tpe);
                numbers(vec![self.add_tree(t) as i32])
            }
            "repeated" => {
                let trees = args.iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let (&elemtpt, elems) = trees.split_last().ok_or("Repeated without its element type")?;
                let t = self.reflect().repeated(elems, elemtpt);
                numbers(vec![self.add_tree(t) as i32])
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
            "let" => {
                let owner = symbol(arg(0)?)?;
                if owner != self.owner && owner != self.macro_owner() {
                    return Err("ValDef.let of an owner other than the splice's");
                }
                let rhs = self.tree(arg(1)? as u32)?;
                let flags = (arg(2)? as u32 as u64) | ((arg(3)? as u32 as u64) << 32);
                let owner = self.owner;
                let (vdef, r) = self.reflect().let_(owner, name, rhs, flags)?;
                numbers(vec![self.add_tree(vdef) as i32, self.add_tree(r) as i32])
            }
            "betaReduce" => {
                let t = self.tree(arg(0)? as u32)?;
                let owner = self.owner;
                let reduced = self.reflect().beta_reduce(t, owner)?;
                numbers(vec![reduced.map_or(-1, |r| self.add_tree(r) as i32)])
            }
            "block" => {
                let trees = args.iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let (&expr, stats) = trees.split_first().ok_or("Block.apply without its expression")?;
                let t = self.reflect().block(stats, expr);
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }
}
