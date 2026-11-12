// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/quoted/PickledQuotes.scala
//! dotty's `TreePickler` and `PositionPickler` over typed trees, for the TASTy of quotes.

mod annotated;
mod applies;
mod cases;
mod defs;
mod names;
mod packages;
mod positions;
mod quote_source;
mod refs;
mod selects;
mod sharing;
mod signed_names;
mod super_accessor_defs;
mod super_accessors;
mod templates;
mod trees;
mod types;
mod unit;
mod variances;

pub use quote_source::QuoteSource;
pub use unit::{FileAttributes, Pickled, pickle_file};

use super_accessors::Accessors;
use unit::File;

use crate::tpd::{TNodeId, Tpd};
use crate::unpickle::QuoteHole;
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use syntax::names::Names;
use tasty::write::{Addr, Name, Pickler, TreeBuffer};
use tasty::{Binder, NameId, SymbolId, TypeId, Universe};

/// Why a tree is outside the subset pickled.
pub type Refusal = &'static str;

pub struct TreePickler<'a> {
    u: &'a mut Universe,
    tpd: &'a Tpd,
    names: &'a Names,
    pickler: Pickler,
    buf: TreeBuffer,
    /// The address of each type pickled, by its sharing key (`sharing::key`).
    types: FxHashMap<Vec<u8>, Addr>,
    keys: FxHashMap<TypeId, Vec<u8>>,
    /// Whether the buffer is a scratch one that a sharing key is written to.
    keying: bool,
    binders: FxHashMap<Binder, Addr>,
    /// The parameter names of each binder pickled.
    params: FxHashMap<Binder, Vec<NameId>>,
    holes: FxHashMap<TNodeId, (u32, Vec<TNodeId>)>,
    /// The definitions of the quote: their addresses, `None` until pickled (dotty's `symRefs`).
    defs: FxHashMap<SymbolId, Option<Addr>>,
    /// The references to definitions not pickled yet (dotty's `forwardSymRefs`).
    forward: FxHashMap<SymbolId, Vec<Addr>>,
    /// The trees that are one tree in dotty's, by the tree pickled first: the class of a module
    /// val's `new` its type's.
    shared: FxHashMap<TNodeId, TNodeId>,
    /// The references to a self pickled as `C.this`, the class name after their tag.
    self_refs: FxHashSet<TNodeId>,
    /// The type tree of each parameter pickled.
    param_tpts: FxHashMap<SymbolId, TNodeId>,
    /// The file pickled, `None` for a quote.
    file: Option<&'a File<'a>>,
    /// The inline calls pickled as references to a class, without their parts.
    call_traces: FxHashSet<TNodeId>,
    /// The parameters of the methods pickled, by the binder of their method's type.
    method_binders: FxHashMap<Binder, Vec<SymbolId>>,
    /// The binders of the method types and type lambdas being pickled.
    open: Vec<Binder>,
    /// The super accessor each super call of a trait selects instead.
    super_calls: FxHashMap<TNodeId, SymbolId>,
    /// The name of each super accessor, and the class and name of those pickled.
    accessor_names: FxHashMap<SymbolId, Name>,
    /// The target name of each super accessor of a member with one.
    accessor_targets: FxHashMap<SymbolId, Name>,
    /// The span of each super accessor's definition, its call's point, and the point of its
    /// member's definition if pickled.
    accessor_spans: FxHashMap<SymbolId, (Span, Span)>,
    /// The trees pickled that have no node, by the id they are registered under, their spans, and
    /// the node whose position theirs follows if not the node pickled next.
    synthetic: Vec<(u32, Span, Option<TNodeId>)>,
    accessors: Accessors,
    /// The type tree pickled now and the annotations its type has pickled: the tree ids of their
    /// `Apply` and `New`, and the type tree.
    type_tree: Option<TNodeId>,
    annots: Vec<(u32, u32, TNodeId)>,
}

/// The TASTy of quote `root`, `holes` its term holes, without positions (dotty's `pickleQuote`).
pub fn pickle_quote(
    cx: (&mut Universe, &Tpd, &Names),
    root: TNodeId,
    holes: &[QuoteHole],
) -> Result<Vec<u8>, Refusal> {
    pickle(cx, root, holes, None, None).map(|(bytes, _)| bytes)
}

/// The TASTy of quote `root` of `source`, `holes` its term holes, with positions.
pub fn pickle_positioned_quote(
    cx: (&mut Universe, &Tpd, &Names),
    root: TNodeId,
    holes: &[QuoteHole],
    source: &QuoteSource,
) -> Result<Vec<u8>, Refusal> {
    pickle(cx, root, holes, Some(source), None).map(|(bytes, _)| bytes)
}

/// The TASTy of `root`: its trees, positions if `source` is given, and the comments and
/// attributes of a file if `file` is; and the super accessors pickled, each its class and name.
fn pickle<'a>(
    (u, tpd, names): (&'a mut Universe, &'a Tpd, &'a Names),
    root: TNodeId,
    holes: &[QuoteHole],
    source: Option<&QuoteSource>,
    file: Option<&'a File<'a>>,
) -> Result<(Vec<u8>, Accessors), Refusal> {
    let mut pickler = Pickler::default();
    let section = pickler.new_section("ASTs");
    let holes = holes.iter().map(|h| (h.node, (h.index, h.args.clone()))).collect();
    let mut p = TreePickler {
        u,
        tpd,
        names,
        pickler,
        buf: TreeBuffer::default(),
        types: FxHashMap::default(),
        keys: FxHashMap::default(),
        keying: false,
        binders: FxHashMap::default(),
        params: FxHashMap::default(),
        holes,
        defs: FxHashMap::default(),
        forward: FxHashMap::default(),
        shared: FxHashMap::default(),
        self_refs: FxHashSet::default(),
        param_tpts: FxHashMap::default(),
        file,
        call_traces: FxHashSet::default(),
        method_binders: FxHashMap::default(),
        open: Vec::new(),
        super_calls: FxHashMap::default(),
        accessor_names: FxHashMap::default(),
        accessor_targets: FxHashMap::default(),
        accessor_spans: FxHashMap::default(),
        synthetic: Vec::new(),
        accessors: Vec::new(),
        type_tree: None,
        annots: Vec::new(),
    };
    p.tree(root)?;
    if !p.forward.is_empty() {
        return Err("a reference to a local definition outside the pickled trees");
    }
    p.buf.compactify();
    let bytes = std::mem::take(&mut p.buf.buf.bytes);
    p.pickler.set_section(section, bytes);
    if let Some(source) = source {
        p.positions(root, source)?;
    }
    if let (Some(file), Some(source)) = (file, source) {
        p.file_sections(&file.attributes, &source.path);
    }
    Ok((p.pickler.assemble(), p.accessors))
}

impl TreePickler<'_> {
    /// A node of tag `tag` with a length, its content written by `f`.
    fn with_length(
        &mut self,
        tag: u8,
        f: &mut dyn FnMut(&mut Self) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        self.byte(tag);
        let len = self.buf.reserve_ref(true);
        f(self)?;
        let end = self.buf.buf.current_addr();
        self.buf.fill_ref(len, end, true);
        Ok(())
    }

    fn byte(&mut self, b: u8) {
        self.buf.buf.write_byte(b);
    }

    fn nat(&mut self, n: u64) {
        self.buf.buf.write_nat(n);
    }
}
