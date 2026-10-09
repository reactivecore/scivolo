//! Lowering typed trees to NIR: ports of dotty's post-typer phases and of Scala Native's NIR
//! generation (docs/research/10-lowering.md, ADRs 0010 and 0011).

mod codegen;
mod compile;
mod definitions;
mod phases;
mod symbols;

pub use compile::compile;
use definitions::Definitions;
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use syntax::source::SourceFile;
use tasty::{SymbolId, Universe};
use typer::tpd::{TNodeId, Tpd};

/// A construct the lowering does not support yet.
#[derive(Debug)]
pub struct Unsupported {
    pub at: Span,
    pub what: String,
}

pub type Result<T> = std::result::Result<T, Unsupported>;

fn unsupported<T>(at: Span, what: impl Into<String>) -> Result<T> {
    Err(Unsupported { at, what: what.into() })
}

/// A `.nir` file: the top-level name it defines and its bytes.
pub struct NirFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// What every phase works on: the unit's trees and names, and the symbols in `u`.
pub struct Lower<'a> {
    pub u: &'a mut Universe,
    pub tpd: &'a mut Tpd,
    pub names: &'a mut Names,
    pub fresh: &'a mut FreshNames,
    pub source: &'a SourceFile,
    defs: Definitions,
    /// The fields `Memoize` made, each with its getter.
    fields: FxHashMap<SymbolId, SymbolId>,
    /// The bridges `Bridges` made and the `$adapted` methods of lambdas.
    bridges: FxHashSet<SymbolId>,
    /// The extension methods entered for the methods of value classes.
    extensions: FxHashMap<SymbolId, SymbolId>,
    /// The fields `LazyVals` made for lazy values, in order.
    lazy_fields: Vec<phases::lazy_vals::LazyField>,
    /// The `$lzyINIT` method of each lazy value's accessor.
    lazy_inits: FxHashMap<SymbolId, SymbolId>,
    /// The fields of lifted local classes holding the values they capture, each with the
    /// constructor parameter setting it.
    proxies: Vec<(SymbolId, SymbolId)>,
    /// The casts phases make that codegen keeps even to a supertype (dotty's `cast`).
    explicit_casts: FxHashSet<TNodeId>,
    /// The local classes lifted out of blocks.
    local_classes: FxHashSet<SymbolId>,
    /// The `while` loops without a condition (dotty's `WhileDo(EmptyTree, _)`) that phases made.
    endless_loops: FxHashSet<TNodeId>,
    /// The accessors `Mixin` made in classes for the values of their traits, without a position.
    mixed_in: FxHashSet<SymbolId>,
    /// The span of each getter's right-hand side as `Getters` made it, its point at its start.
    getter_rhs: FxHashMap<SymbolId, Span>,
    /// The field and constructor parameter of each class holding its enclosing instance.
    outers: FxHashMap<SymbolId, (SymbolId, SymbolId)>,
}

/// The NIR files of typed unit `root`, one per top-level name.
pub fn lower(
    u: &mut Universe,
    tpd: &mut Tpd,
    root: TNodeId,
    names: &mut Names,
    fresh: &mut FreshNames,
    source: &SourceFile,
) -> Result<Vec<NirFile>> {
    let defs = Definitions::new(u).map_err(|what| Unsupported { at: Span::NONE, what })?;
    let mut l = Lower {
        u,
        tpd,
        names,
        fresh,
        source,
        defs,
        fields: FxHashMap::default(),
        bridges: FxHashSet::default(),
        extensions: FxHashMap::default(),
        lazy_fields: Vec::new(),
        lazy_inits: FxHashMap::default(),
        endless_loops: FxHashSet::default(),
        proxies: Vec::new(),
        local_classes: FxHashSet::default(),
        explicit_casts: FxHashSet::default(),
        mixed_in: FxHashSet::default(),
        getter_rhs: FxHashMap::default(),
        outers: FxHashMap::default(),
    };
    phases::run(&mut l, root)?;
    codegen::unit(&mut l, root)
}
