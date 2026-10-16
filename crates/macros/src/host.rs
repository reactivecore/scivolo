//! The frontend's side of a running macro: the typer's trees and tables, lent for the run, and
//! the expressions and types the macro holds by number.

use crate::tasty_string;
use syntax::names::Names;
use tasty::{SymbolId, TypeId, Universe};
use typer::inlining::Quoted;
use typer::tpd::{TNodeId, Tpd};
use typer::unpickle::{Quote, quote, quoted_type};

/// The universe, trees and names the typer lends the expansion.
pub type Tables<'l> = (&'l mut Universe, &'l mut Tpd, &'l mut Names);

/// A quote unpickled for the macro: its tree, its spliced types and, per term hole, the
/// placeholder, the pickled index, the arguments' expressions and the filling expression.
struct Unpickled {
    root: TNodeId,
    spliced: Vec<(SymbolId, TypeId)>,
    holes: Vec<(TNodeId, u32, Vec<u32>, Option<u32>)>,
}

enum Expr {
    Tree(TNodeId),
    Quote(Unpickled),
}

pub struct Host {
    pub u: Universe,
    pub tpd: Tpd,
    pub names: Names,
    /// The owner of the splice expanded.
    owner: SymbolId,
    exprs: Vec<Expr>,
    types: Vec<TypeId>,
    /// Why the expansion is refused, if it is.
    pub refusal: Option<String>,
}

impl Host {
    /// A host of the trees and tables of `lent`, which it takes, leaving empty ones.
    pub fn new((u, tpd, names): Tables<'_>, owner: SymbolId) -> Host {
        Host {
            u: std::mem::replace(u, Universe::empty()),
            tpd: std::mem::take(tpd),
            names: std::mem::take(names),
            owner,
            exprs: Vec::new(),
            types: Vec::new(),
            refusal: None,
        }
    }

    /// Hands the trees and tables back to `lent`.
    pub fn restore(&mut self, (u, tpd, names): Tables<'_>) {
        std::mem::swap(u, &mut self.u);
        std::mem::swap(tpd, &mut self.tpd);
        std::mem::swap(names, &mut self.names);
    }

    pub fn add_tree(&mut self, t: TNodeId) -> u32 {
        self.exprs.push(Expr::Tree(t));
        self.exprs.len() as u32 - 1
    }

    pub fn add_type(&mut self, t: TypeId) -> u32 {
        self.types.push(t);
        self.types.len() as u32 - 1
    }

    /// The expressions the macro made or was given, each quote's holes filled; a refusal if
    /// one is open.
    pub fn quoted(&self) -> Result<Vec<Quoted>, &'static str> {
        let quoted = |e: &Expr| match e {
            Expr::Tree(t) => Ok(Quoted::Tree(*t)),
            Expr::Quote(q) => {
                let holes: Option<Vec<_>> =
                    q.holes.iter().map(|&(node, _, _, filled)| Some((node, filled?))).collect();
                let holes = holes.ok_or("a quote whose hole the macro left open")?;
                Ok(Quoted::Quote { root: q.root, spliced: q.spliced.clone(), holes })
            }
        };
        self.exprs.iter().map(quoted).collect()
    }

    /// The bytes of `pickled` and the types numbered `types`.
    fn decoded(&self, pickled: &str, types: &[u32]) -> Result<(Vec<u8>, Vec<TypeId>), &'static str> {
        let bytes = tasty_string::decode(pickled).ok_or("a quote not in Base64")?;
        let types: Option<Vec<TypeId>> = types.iter().map(|&t| self.types.get(t as usize).copied()).collect();
        Ok((bytes, types.ok_or("a quote of an unknown type")?))
    }
}

/// The refusals of the natives become `Unsupported` exceptions, which record them through
/// `refuse`.
impl vm::QuoteHost for Host {
    fn unpickle_expr(&mut self, pickled: &str, types: &[u32]) -> Result<u32, &'static str> {
        let (bytes, types) = self.decoded(pickled, types)?;
        let lent = (&mut self.u, &mut self.tpd, &mut self.names);
        let Quote { root, spliced, holes } = quote(lent, bytes, self.owner, &types)?;
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
        let t = quoted_type(lent, bytes, self.owner, &types)?;
        Ok(self.add_type(t))
    }

    fn hole_count(&self, quote: u32) -> u32 {
        match self.exprs.get(quote as usize) {
            Some(Expr::Quote(q)) => q.holes.len() as u32,
            _ => 0,
        }
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

    fn refuse(&mut self, what: &str) {
        self.refusal.get_or_insert_with(|| what.to_owned());
    }
}
