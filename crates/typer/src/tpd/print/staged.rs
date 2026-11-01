//! Calls of `scala.quoted.runtime.Expr.quote` and `splice` as dotty's `Quote` and `Splice` trees.

use super::{Printer, types};
use crate::tpd::{TNodeId, Tree};
use std::fmt::Write;

impl Printer<'_> {
    /// Prints `id` as a `Quote` or `Splice` tree if it is a call of `quote[T](e)` or
    /// `splice[T](e)`; whether it is one.
    pub fn staged(&mut self, id: TNodeId) -> bool {
        let Tree::Apply { fun, args } = *self.tpd.tree(id) else { return false };
        let Tree::TypeApply { fun: method, .. } = *self.tpd.tree(fun) else { return false };
        let (Some(sym), &[arg]) = (self.tpd.sym(method), self.tpd.items(args)) else { return false };
        let quote = match self.u.full_name(sym).as_str() {
            "scala.quoted.runtime.Expr$.quote" => true,
            "scala.quoted.runtime.Expr$.splice" => false,
            _ => return false,
        };
        let node = self.tpd.get(id);
        let (span, tpe) = (self.span(node.span), node.tpe);
        let class = if quote { "Quote" } else { "Splice" };
        write!(self.out, "({class} {span}").unwrap();
        if let Some(t) = tpe {
            write!(self.out, " :{}", types::tpe(self.u, t)).unwrap();
        }
        self.node(arg);
        self.out.push_str(if quote { " [])" } else { ")" });
        true
    }
}
