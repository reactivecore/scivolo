//! The host as the VM's natives of `QuoteHost` reach it.

use super::{Expr, Host, Unpickled};
use typer::unpickle::{Quote, quote, quoted_type};

/// The refusals of the natives become `Unsupported` exceptions, which record them through
/// `refuse`.
impl vm::QuoteHost for Host {
    fn unpickle_expr(&mut self, pickled: &str, types: &[u32]) -> Result<u32, &'static str> {
        let (bytes, types) = self.decoded(pickled, types)?;
        let lent = (&mut self.u, &mut self.tpd, &mut self.names);
        let Quote { root, spliced, holes } = match quote(lent, bytes, self.owner, &types)? {
            typer::unpickle::Unpickled::Quote(q) => q,
            typer::unpickle::Unpickled::Pattern(p) => return Ok(self.add_pattern(p)),
        };
        let holes = holes
            .into_iter()
            .map(|h| (h.node, h.index, h.args.into_iter().map(|a| self.add_tree(a)).collect(), None))
            .collect();
        self.exprs.push(Expr::Quote(Unpickled { root, spliced, holes }));
        Ok(self.exprs.len() as u32 - 1)
    }

    fn unpickle_type(&mut self, pickled: &str, types: &[u32]) -> Result<u32, &'static str> {
        let (bytes, types) = self.decoded(pickled, types)?;
        let lent = (&mut self.u, &mut self.tpd, &mut self.names);
        Ok(match quoted_type(lent, bytes, self.owner, &types)? {
            Ok(t) => self.add_type(t),
            Err(p) => self.add_pattern(p),
        })
    }

    fn hole_count(&self, quote: u32) -> u32 {
        let Some(Expr::Quote(q)) = self.exprs.get(quote as usize) else { return 0 };
        q.holes.len() as u32
    }

    fn hole(&self, quote: u32, hole: u32) -> Option<vm::Hole> {
        let Some(Expr::Quote(q)) = self.exprs.get(quote as usize) else { return None };
        let (_, index, args, _) = q.holes.get(hole as usize)?;
        Some((*index, args.iter().map(|&a| Ok(a)).collect()))
    }

    fn fill_hole(&mut self, quote: u32, hole: u32, expr: u32) -> Result<(), &'static str> {
        let known = (expr as usize) < self.exprs.len();
        if let Some(Expr::Quote(q)) = self.exprs.get_mut(quote as usize)
            && let Some(h) = q.holes.get_mut(hole as usize)
            && known
        {
            h.3 = Some(expr);
            return Ok(());
        }
        Err("a quote hole filled with an unknown expression")
    }

    fn match_pattern(&mut self, scrutinee: u32, pattern: u32) -> Result<u32, &'static str> {
        self.match_expr(scrutinee, pattern)
    }

    fn matched(&self, i: u32) -> Option<Result<u32, u32>> {
        self.matched.get(i as usize).copied()
    }

    fn reflect(&mut self, op: &str, args: &[i32], text: &str) -> Result<vm::Reflected, &'static str> {
        self.run_reflect(op, args, text)
    }

    fn refuse(&mut self, what: &str) {
        self.refusal.get_or_insert_with(|| what.to_owned());
    }
}
