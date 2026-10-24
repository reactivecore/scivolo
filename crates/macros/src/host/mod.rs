//! The frontend's side of a running macro: the typer's trees and tables, lent for the run, and
//! the expressions and types the macro holds by number.

mod natives;

use crate::Tables;
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use syntax::source::SourceFile;
use tasty::{SymbolId, TypeId, Universe};
use typer::inlining::Quoted;
use typer::tpd::{TNodeId, Tpd};
use typer::unpickle::QuotePattern;

/// A quote unpickled for the macro: its tree, spliced types and term holes (placeholder, index,
/// arguments' and filling expressions).
struct Unpickled {
    root: TNodeId,
    spliced: Vec<(SymbolId, TypeId)>,
    holes: Vec<(TNodeId, u32, Vec<u32>, Option<u32>)>,
}

enum Expr {
    Tree(TNodeId),
    Quote(Unpickled),
}

/// The bit that marks the number of a quote pattern among those of expressions and types.
pub const PATTERN: u32 = 1 << 30;

pub struct Host {
    pub u: Universe,
    pub tpd: Tpd,
    pub names: Names,
    /// The owner of the splice expanded.
    pub owner: SymbolId,
    /// The span of the macro expansion, in `sources`.
    pub expansion: Span,
    pub sources: Vec<SourceFile>,
    pub typer: crate::reflect::FromTyper,
    exprs: Vec<Expr>,
    types: Vec<TypeId>,
    pub patterns: Vec<QuotePattern>,
    /// What the last quote pattern matched: expressions `Ok`, types `Err`.
    pub matched: Vec<Result<u32, u32>>,
    /// Why the expansion is refused, if it is.
    pub refusal: Option<String>,
    pub made: crate::reflect::Made,
    /// The errors the macro reported, with their spans.
    pub reported: Vec<(Span, String)>,
    /// The expressions of the definitions a macro annotation returned.
    pub definitions: Option<Vec<u32>>,
    /// Whether a macro annotation runs, whose splice owner is `owner` itself.
    pub annotating: bool,
    pub fresh: FreshNames,
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
            typer: Default::default(),
            exprs: Vec::new(),
            types: Vec::new(),
            patterns: Vec::new(),
            matched: Vec::new(),
            refusal: None,
            made: Default::default(),
            reported: Vec::new(),
            definitions: None,
            annotating: false,
            fresh: FreshNames::default(),
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
        match self.exprs.get(expr as usize).ok_or("an Expr the frontend did not make")? {
            Expr::Tree(t) | Expr::Quote(Unpickled { root: t, .. }) => Ok(*t),
        }
    }

    /// Quote pattern `n`, or the pattern of tree `n` or of quote `n` without holes.
    pub fn pattern(&self, n: u32) -> Option<QuotePattern> {
        match self.exprs.get(n as usize) {
            _ if n & PATTERN != 0 => self.patterns.get((n & !PATTERN) as usize).cloned(),
            Some(Expr::Tree(t)) => Some(QuotePattern::of(*t, &[])),
            Some(Expr::Quote(q)) if q.holes.is_empty() => Some(QuotePattern::of(q.root, &q.spliced)),
            _ => None,
        }
    }

    /// The hole placeholders of the quotes with the trees that fill them, if filled.
    pub fn hole_fills(&self) -> Vec<(TNodeId, Option<TNodeId>)> {
        let holes = self.exprs.iter().flat_map(|e| match e {
            Expr::Quote(q) => q.holes.as_slice(),
            Expr::Tree(_) => &[],
        });
        holes.map(|&(node, _, _, filled)| (node, filled.and_then(|e| self.tree(e).ok()))).collect()
    }

    /// The spliced type symbols of the quotes with the types they stand for.
    pub fn spliced_types(&self) -> Vec<(SymbolId, TypeId)> {
        let quotes = self.exprs.iter().filter_map(|e| match e {
            Expr::Quote(q) => Some(&q.spliced),
            Expr::Tree(_) => None,
        });
        quotes.flatten().copied().collect()
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
                    q.holes.iter().map(|&(h, _, _, filled)| Some((h, filled?))).collect();
                let holes = holes.ok_or("a quote whose hole the macro left open")?;
                Ok(Quoted::Quote { root: q.root, spliced: q.spliced.clone(), holes })
            }
        };
        self.exprs.iter().map(quoted).collect()
    }
}
