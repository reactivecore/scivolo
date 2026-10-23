// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The terms of `quotes.reflect`.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use std::collections::HashMap;
use typer::tpd::{TNodeId, Tpd, map_children};

impl Host {
    /// Runs term operation `op` on `args`.
    pub fn run_term(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
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
                let trees = self.trees(args)?;
                let (&elemtpt, elems) = trees.split_last().ok_or("Repeated without its element type")?;
                let t = self.reflect().repeated(elems, elemtpt);
                numbers(vec![self.add_tree(t) as i32])
            }
            "ref" => {
                let t = self.reflect().ref_(symbol(arg(0)?)?)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "identRef" => {
                let tpe = self.type_of(arg(0)? as u32)?;
                let tasty::Type::Sym(_, s) = *self.u.types().get(tpe) else {
                    return Err("Ident.apply of a reference the frontend did not resolve");
                };
                let t = self.reflect().ref_(s)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "selectUnique" => {
                let qual = self.tree(arg(0)? as u32)?;
                let tpe = self.tpd.tpe(qual);
                let found = self.u.member_of_type(tpe, text);
                let terms: Vec<_> = found.into_iter().filter(|&s| self.u.symbol(s).kind.is_term()).collect();
                let [m] = terms[..] else { return Err("Select.unique of no or an overloaded member") };
                let t = self.reflect().select(qual, m)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "selectOverloaded" => self.select_overloaded(args, text),
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
            "match" | "apply" | "typed" => {
                let trees = self.trees(args)?;
                let t = match op {
                    "match" => self.reflect().match_(trees[0], &trees[1..]),
                    "typed" if trees.len() == 2 => self.reflect().typed(trees[0], trees[1]),
                    "typed" => return Err("Typed.apply without its type tree"),
                    _ => self.reflect().apply(trees[0], &trees[1..])?,
                };
                numbers(vec![self.add_tree(t) as i32])
            }
            "caseDef" => {
                let guard = (arg(1)? >= 0).then(|| self.tree(arg(1)? as u32)).transpose()?;
                let (pat, body) = (self.tree(arg(0)? as u32)?, self.tree(arg(2)? as u32)?);
                let t = self.reflect().case_def(pat, guard, body);
                numbers(vec![self.add_tree(t) as i32])
            }
            "wildcard" => {
                let t = self.reflect().wildcard();
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
                let (vdef, r) = self.reflect().let_(owner, text, rhs, flags)?;
                numbers(vec![self.add_tree(vdef) as i32, self.add_tree(r) as i32])
            }
            "betaReduce" => {
                let t = self.tree(arg(0)? as u32)?;
                let fills: HashMap<_, _> =
                    self.hole_fills().into_iter().filter_map(|(h, f)| Some((h, f?))).collect();
                let filled = filled(&mut self.tpd, &fills, t);
                let owner = self.owner;
                let reduced = self.reflect().beta_reduce(filled, owner)?;
                numbers(vec![reduced.map_or(-1, |r| self.add_tree(r) as i32)])
            }
            "block" => {
                let trees = args.iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let (&expr, stats) = trees.split_first().ok_or("Block.apply without its expression")?;
                let t = self.reflect().block(stats, expr)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }
}

/// `t` with the hole placeholders in it replaced by the trees in `fills` filling them, the trees
/// on their paths copied.
fn filled(tpd: &mut Tpd, fills: &HashMap<TNodeId, TNodeId>, t: TNodeId) -> TNodeId {
    if let Some(&f) = fills.get(&t) {
        return filled(tpd, fills, f);
    }
    let kids = tpd.children(t).to_vec();
    let new: Vec<_> = kids.iter().map(|&c| filled(tpd, fills, c)).collect();
    if new == kids {
        return t;
    }
    let n = tpd.node(tpd.tree(t).clone(), tpd.span(t), tpd.get(t).tpe, tpd.sym(t));
    let mut new = new.into_iter();
    map_children(tpd, n, &mut |_, c| new.next().unwrap_or(c));
    n
}

impl Host {
    /// The trees of expressions `args`, at least one.
    fn trees(&self, args: &[i32]) -> Result<Vec<TNodeId>, &'static str> {
        let trees = args.iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
        if trees.is_empty() {
            return Err("a quotes.reflect operation without its arguments");
        }
        Ok(trees)
    }
}
