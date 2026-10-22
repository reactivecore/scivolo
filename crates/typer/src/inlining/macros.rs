//! What expanding a macro asks of the one who runs macro implementations (ADR 0029).

use super::Lent;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{SymbolId, TypeId};

/// An argument of a macro implementation: a quoted expression, a type or the `Quotes`.
#[derive(Clone, Copy, Debug)]
pub enum MacroArg {
    Expr(TNodeId),
    Type(TypeId),
    Quotes,
}

/// A call of macro implementation `method` with `args`, from a splice owned by `owner`.
pub struct MacroCall {
    pub method: SymbolId,
    pub args: Vec<MacroArg>,
    pub owner: SymbolId,
    pub span: Span,
    /// The span of the outermost inline call expanded (dotty's `Position.ofMacroExpansion`).
    pub expansion: Span,
}

/// An expression a macro made or was given, by number.
#[derive(Clone, Debug)]
pub enum Quoted {
    /// A tree of the splice's arguments or of a hole's.
    Tree(TNodeId),
    /// An unpickled quote: its tree, the types its spliced type definitions stand for, and the
    /// expression filling each of its term holes.
    Quote { root: TNodeId, spliced: Vec<(SymbolId, TypeId)>, holes: Vec<(TNodeId, u32)> },
}

/// What a macro expansion made: its expressions and the number of the one returned.
pub struct Expanded {
    pub exprs: Vec<Quoted>,
    pub result: u32,
}

pub enum MacroError {
    /// The macro needs what the frontend does not support.
    Unsupported(String),
    /// The macro failed, with the message dotty reports.
    Failed(String),
    /// The macro reported errors (`report.error`, `errorAndAbort`): their spans and messages.
    Reported(Vec<(Span, String)>),
    /// The macro threw `StopMacroExpansion` without reporting an error.
    Aborted,
}

/// Runs macro implementations.
pub trait Macros {
    fn expand(&mut self, lent: Lent<'_>, call: &MacroCall) -> Result<Expanded, MacroError>;
}
