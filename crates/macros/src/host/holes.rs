//! The holes of the macro's quotes that implicit searches run in.

use super::{Expr, Host};
use typer::tpd::TNodeId;

impl Host {
    /// The roots of the quotes with an open hole, and the first open one, which the macro runs in.
    pub fn running_holes(&self) -> Vec<(TNodeId, TNodeId)> {
        let open = |e: &Expr| match e {
            Expr::Quote(q) => q.holes.iter().find(|h| h.filled.is_none()).map(|h| (q.root, h.placeholder)),
            _ => None,
        };
        self.exprs.iter().filter_map(open).collect()
    }

    /// The root of quote `quote` and the placeholder of its hole `hole`.
    pub fn hole_at(&self, quote: i32, hole: i32) -> Result<(TNodeId, TNodeId), &'static str> {
        let found = match self.exprs.get(quote as u32 as usize) {
            Some(Expr::Quote(q)) => q.holes.get(hole as u32 as usize).map(|h| (q.root, h.placeholder)),
            _ => None,
        };
        found.ok_or("an implicit search in a hole the frontend did not make")
    }
}
