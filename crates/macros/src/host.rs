//! The frontend's side of a running macro: the typer's trees and tables, lent for the run, and
//! the expressions and types the macro holds by number.

use syntax::ast::Span;
use syntax::names::Names;
use syntax::source::SourceFile;
use tasty::{SymbolId, TypeId, Universe};
use typer::inlining::{Quoted, quote_match};
use typer::tpd::{TNodeId, Tpd};
use typer::unpickle::{Quote, QuotePattern, quote, quoted_type};

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

/// The bit that marks the number of a quote pattern among those of expressions.
const PATTERN: u32 = 1 << 30;

pub struct Host {
    pub u: Universe,
    pub tpd: Tpd,
    pub names: Names,
    /// The owner of the splice expanded.
    pub owner: SymbolId,
    /// The span of the macro expansion, in `sources`.
    pub expansion: Span,
    pub sources: Vec<SourceFile>,
    exprs: Vec<Expr>,
    types: Vec<TypeId>,
    patterns: Vec<QuotePattern>,
    /// What the last quote pattern matched: expressions `Ok`, types `Err`.
    matched: Vec<Result<u32, u32>>,
    /// Why the expansion is refused, if it is.
    pub refusal: Option<String>,
    pub made: crate::symbols::Made,
}

impl Host {
    /// A host of the trees and tables of `lent`, which it takes, leaving empty ones.
    pub fn new(
        (u, tpd, names): Tables<'_>,
        owner: SymbolId,
        (expansion, sources): (Span, Vec<SourceFile>),
    ) -> Host {
        Host {
            u: std::mem::replace(u, Universe::empty()),
            tpd: std::mem::take(tpd),
            names: std::mem::take(names),
            owner,
            expansion,
            sources,
            exprs: Vec::new(),
            types: Vec::new(),
            patterns: Vec::new(),
            matched: Vec::new(),
            refusal: None,
            made: Default::default(),
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

    /// The tree of expression `expr`, a quote's with its holes open.
    pub fn tree(&self, expr: u32) -> Result<TNodeId, &'static str> {
        match self.exprs.get(expr as usize) {
            Some(Expr::Tree(t)) => Ok(*t),
            Some(Expr::Quote(q)) => Ok(q.root),
            None => Err("an Expr the frontend did not make"),
        }
    }

    pub fn type_of(&self, id: u32) -> Result<TypeId, &'static str> {
        self.types.get(id as usize).copied().ok_or("a Type the frontend did not make")
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
}

/// The refusals of the natives become `Unsupported` exceptions, which record them through
/// `refuse`.
impl vm::QuoteHost for Host {
    fn unpickle_expr(&mut self, pickled: &str, types: &[u32]) -> Result<u32, &'static str> {
        let (bytes, types) = self.decoded(pickled, types)?;
        let lent = (&mut self.u, &mut self.tpd, &mut self.names);
        let Quote { root, spliced, holes } = match quote(lent, bytes, self.owner, &types)? {
            typer::unpickle::Unpickled::Quote(q) => q,
            typer::unpickle::Unpickled::Pattern(p) => {
                self.patterns.push(p);
                return Ok(PATTERN | (self.patterns.len() as u32 - 1));
            }
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

    fn match_pattern(&mut self, scrutinee: u32, pattern: u32) -> Result<u32, &'static str> {
        let tpe = self.tpd.tpe(self.tree(scrutinee)?);
        let unknown = "a quote pattern the frontend did not unpickle";
        let pattern = (pattern & PATTERN != 0).then_some(pattern & !PATTERN).ok_or(unknown)?;
        let pattern = self.patterns.get(pattern as usize).ok_or(unknown)?;
        let Some(types) = quote_match(&mut self.u, tpe, pattern) else { return Ok(u32::MAX) };
        let types: Vec<u32> = types.into_iter().map(|t| self.add_type(t)).collect();
        self.matched = types.into_iter().map(Err).chain([Ok(scrutinee)]).collect();
        Ok(self.matched.len() as u32)
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
