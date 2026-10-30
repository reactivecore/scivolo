//! What expanding a macro asks of the one who runs macro implementations (ADR 0029).

use super::Lent;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use syntax::diag::Level;
use tasty::{SymbolId, TypeId};

/// An argument of a macro implementation: a quoted expression, a type, the `Quotes`, a literal or
/// the elements of a repeated parameter.
#[derive(Clone, Debug)]
pub enum MacroArg {
    Expr(TNodeId),
    Type(TypeId),
    Quotes,
    Literal(Constant),
    Varargs(Vec<MacroArg>),
}

impl MacroArg {
    /// The trees of the quoted expressions of `args`.
    pub fn exprs(args: &[MacroArg]) -> Vec<TNodeId> {
        let one = |a: &MacroArg| match a {
            MacroArg::Expr(t) => vec![*t],
            MacroArg::Varargs(elems) => MacroArg::exprs(elems),
            _ => Vec::new(),
        };
        args.iter().flat_map(one).collect()
    }
}

/// A call of macro implementation `method` on static object `receiver` with `args`, from a
/// splice owned by `owner`; its result applied to the `Quotes` `context` times.
pub struct MacroCall {
    pub method: SymbolId,
    pub receiver: SymbolId,
    pub context: u32,
    pub args: Vec<MacroArg>,
    pub owner: SymbolId,
    pub span: Span,
    /// The span of the outermost inline call expanded (dotty's `Position.ofMacroExpansion`).
    pub expansion: Span,
    /// The implicit searches of earlier runs of the expansion, with the arguments found.
    pub searched: Vec<(TypeId, Option<TNodeId>)>,
    /// Whether it expands while the typer runs (dotty's `CompilationInfo.isWhileTyping`).
    pub while_typing: bool,
}

/// The call of `transform` of macro annotation `annotation`, a typed `new`, on `definition` and
/// its `companion`, owned by `owner` (dotty's `MacroAnnotations.callMacro`).
pub struct AnnotationCall {
    pub annotation: TNodeId,
    pub definition: TNodeId,
    pub companion: Option<TNodeId>,
    pub owner: SymbolId,
    pub span: Span,
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

/// What a macro expansion made: its expressions and the numbers of those returned, one for a
/// splice, the definitions for a macro annotation; the warnings it reported.
pub struct Expanded {
    pub exprs: Vec<Quoted>,
    pub results: Vec<u32>,
    pub warnings: Reports,
}

/// The errors and warnings a macro reported (`quotes.reflect.report`), with their spans.
pub type Reports = Vec<(Level, Span, String)>;

pub enum MacroError {
    /// The macro needs what the frontend does not support.
    Unsupported(String),
    /// The macro failed, with the message dotty reports.
    Failed(String),
    /// The macro reported errors (`report.error`, `errorAndAbort`), and maybe warnings.
    Reported(Reports),
    /// The macro threw `StopMacroExpansion` without reporting an error.
    Aborted,
    /// The macro searched an implicit argument of a type `searched` lacks; the expansion runs
    /// again once the typer has searched it.
    Search(TypeId),
}

/// The typed annotations of the unit's definitions, by symbol.
pub type Annotations = rustc_hash::FxHashMap<SymbolId, Vec<TNodeId>>;

/// Runs macro implementations, which see the annotations `annotations`.
pub trait Macros {
    fn expand(
        &mut self,
        lent: Lent<'_>,
        call: &MacroCall,
        annotations: &Annotations,
    ) -> Result<Expanded, MacroError>;

    /// Runs the `transform` of a macro annotation.
    fn expand_annotation(
        &mut self,
        lent: Lent<'_>,
        call: &AnnotationCall,
        annotations: &Annotations,
    ) -> Result<Expanded, MacroError>;
}
