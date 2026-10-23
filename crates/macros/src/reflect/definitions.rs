// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions of `quotes.reflect`: the annotations and trees of symbols, the bodies of
//! classes and the clauses of methods.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// Runs definition operation `op` on `args`.
    pub fn run_definition(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        match op {
            "annotations" => {
                let (s, cls) = (symbol(arg(args, 0)?)?, symbol(arg(args, 1)?).ok());
                if self.definition(s).is_none() {
                    let owner = self.u.symbol(s).owner.map(|o| self.u.full_name(o));
                    let universal = matches!(owner.as_deref(), Some("java.lang.Object" | "scala.Any"));
                    return match cls {
                        Some(_) if universal => numbers(Vec::new()),
                        _ => Err("the annotations of a symbol defined outside the unit"),
                    };
                }
                let mut trees = self.typer.annotations.get(&s).cloned().unwrap_or_default();
                if let Some(cls) = cls {
                    trees.retain(|&t| self.u.class_symbol(self.tpd.tpe(t)) == Some(cls));
                }
                numbers(trees.into_iter().map(|t| self.add_tree(t) as i32).collect())
            }
            "symbolTree" => {
                let t = self.definition(symbol(arg(args, 0)?)?);
                numbers(vec![t.map_or(-1, |t| self.add_tree(t) as i32)])
            }
            "classBody" => {
                let t = self.tree(arg(args, 0)? as u32)?;
                let T::TypeDef { rhs, .. } = *self.tpd.tree(t) else { return Err(NO_CLASS) };
                let T::Template { body, .. } = *self.tpd.tree(rhs) else { return Err(NO_CLASS) };
                let stats = self.tpd.items(body).to_vec();
                numbers(stats.into_iter().map(|s| self.add_tree(s) as i32).collect())
            }
            "defClauses" => {
                let t = self.tree(arg(args, 0)? as u32)?;
                let T::DefDef { paramss, .. } = *self.tpd.tree(t) else {
                    return Err("DefDef.paramss of another tree");
                };
                let clauses: Vec<Vec<TNodeId>> =
                    self.tpd.clause_items(paramss).iter().map(|&l| self.tpd.items(l).to_vec()).collect();
                let mut answer = vec![clauses.len() as i32];
                for c in &clauses {
                    let is_type = c.first().is_some_and(|&p| matches!(self.tpd.tree(p), T::TypeDef { .. }));
                    answer.extend([is_type as i32, c.len() as i32]);
                }
                answer.extend(clauses.into_iter().flatten().map(|p| self.add_tree(p) as i32));
                numbers(answer)
            }
            _ => Err(UNKNOWN),
        }
    }

    /// The definition of `s` among the typer's trees (dotty's `Symbol.tree`), if it has one.
    pub fn definition(&self, s: SymbolId) -> Option<TNodeId> {
        let defines = |d: TNodeId| {
            self.tpd.sym(d) == Some(s)
                && matches!(self.tpd.tree(d), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. })
        };
        (0..self.tpd.len()).rev().map(TNodeId::new).find(|&d| defines(d))
    }
}

const NO_CLASS: &str = "ClassDef.body of another tree";
