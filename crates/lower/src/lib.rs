//! Lowering typed trees to NIR: ports of dotty's post-typer phases and of Scala Native's NIR
//! generation (docs/research/10-lowering.md, ADRs 0010 and 0011).

mod codegen;
mod compile;
mod definitions;
mod phases;
mod symbols;

pub use compile::{Refusal, compile};
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
    /// The `$lzyINIT` method and field of each class lazy value's accessor.
    lazy_members: FxHashMap<SymbolId, phases::lazy_vals::LazyMembers>,
    /// The accessors of local lazy values, before and after `LambdaLift`.
    local_lazy_accessors: FxHashSet<SymbolId>,
    /// The fields of lifted local classes holding the values they capture, each with the
    /// constructor parameter setting it.
    proxies: Vec<(SymbolId, SymbolId)>,
    /// The field of each lifted local class holding a value it captures, by class and value.
    captured_fields: FxHashMap<(SymbolId, SymbolId), SymbolId>,
    /// The methods of local traits taking the local values they use first: each one's type
    /// before, and the values with the parameters standing for them.
    trait_methods: FxHashMap<SymbolId, (tasty::TypeId, Vec<(SymbolId, SymbolId)>)>,
    /// The calls `LambdaLift` made of references to methods without parameter lists, which
    /// dotty's `LetOverApply` sees as references.
    applied_references: FxHashSet<TNodeId>,
    /// The methods of local traits each class gets forwarders to, decided before `LambdaLift`.
    trait_forwards: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// The casts phases make that codegen keeps even to a supertype (dotty's `cast`).
    explicit_casts: FxHashSet<TNodeId>,
    /// The casts of `null` codegen keeps: unboxed nulls of value classes.
    null_casts: FxHashSet<TNodeId>,
    /// The local classes lifted out of blocks, each with its owner before.
    local_classes: FxHashMap<SymbolId, SymbolId>,
    /// The `while` loops without a condition (dotty's `WhileDo(EmptyTree, _)`) that phases made.
    endless_loops: FxHashSet<TNodeId>,
    /// The accessors `Mixin` made in classes for the values of their traits, without a position.
    mixed_in: FxHashSet<SymbolId>,
    /// The specialized `apply` methods `SpecializeFunctions` made, without a position.
    specialized: FxHashSet<SymbolId>,
    /// The names of the field and `$lzyINIT` method of each lazy value a class gets from a trait
    /// member, by class and member.
    mixed_in_lazy_names: FxHashMap<(SymbolId, SymbolId), (String, String)>,
    /// The method each forwarder `Mixin` made calls, but for a JDK trait's.
    forwarded: FxHashMap<SymbolId, SymbolId>,
    /// The span of each getter's right-hand side as `Getters` made it, its point at its start.
    getter_rhs: FxHashMap<SymbolId, Span>,
    /// The field, unless dropped, and constructor parameter of each class holding its enclosing
    /// instance.
    outers: FxHashMap<SymbolId, (Option<SymbolId>, SymbolId)>,
    /// The accessor of the enclosing instance of each class needing one.
    outer_accessors: FxHashMap<SymbolId, SymbolId>,
    /// The positions of the methods whose symbols dotty places elsewhere than their definitions.
    coords: FxHashMap<SymbolId, Span>,
    /// The local functions of classes in local classes that read values captured by an enclosing
    /// local class, each with that class, which they are lifted into (dotty's `narrowLogicOwner`).
    narrowed: FxHashMap<SymbolId, SymbolId>,
    /// The term each method `LambdaLift` lifted was defined in.
    lifted_from: FxHashMap<SymbolId, SymbolId>,
    /// The member each super accessor of the unit accesses, whether through a named mixin.
    super_accessors: FxHashMap<SymbolId, (SymbolId, bool)>,
    /// The span of the outermost `Inlined` tree without bindings each tree was the expansion of.
    inlined_spans: FxHashMap<TNodeId, Span>,
    /// The blocks `drop_inlined` made of `Inlined` trees with bindings.
    inlined_blocks: FxHashSet<TNodeId>,
    /// The objects the unit refers to whose creation runs code.
    impure_modules: FxHashSet<SymbolId>,
    /// The method or constructor whose trees `Erasure` types, owning the values it defines.
    erasure_owner: Option<SymbolId>,
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
        lazy_members: FxHashMap::default(),
        local_lazy_accessors: FxHashSet::default(),
        endless_loops: FxHashSet::default(),
        proxies: Vec::new(),
        captured_fields: FxHashMap::default(),
        trait_methods: FxHashMap::default(),
        trait_forwards: FxHashMap::default(),
        applied_references: FxHashSet::default(),
        local_classes: FxHashMap::default(),
        explicit_casts: FxHashSet::default(),
        null_casts: FxHashSet::default(),
        mixed_in: FxHashSet::default(),
        specialized: FxHashSet::default(),
        mixed_in_lazy_names: FxHashMap::default(),
        forwarded: FxHashMap::default(),
        getter_rhs: FxHashMap::default(),
        outers: FxHashMap::default(),
        outer_accessors: FxHashMap::default(),
        coords: FxHashMap::default(),
        narrowed: FxHashMap::default(),
        lifted_from: FxHashMap::default(),
        super_accessors: FxHashMap::default(),
        inlined_spans: FxHashMap::default(),
        inlined_blocks: FxHashSet::default(),
        impure_modules: FxHashSet::default(),
        erasure_owner: None,
    };
    phases::run(&mut l, root)?;
    codegen::unit(&mut l, root)
}
