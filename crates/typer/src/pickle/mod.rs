// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/quoted/PickledQuotes.scala
//! dotty's `TreePickler` and `PositionPickler` over typed trees, for the TASTy of quotes.

mod cases;
mod defs;
mod names;
mod positions;
mod refs;
mod sharing;
mod trees;
mod types;

pub use positions::QuoteSource;

use crate::tpd::{TNodeId, Tpd};
use crate::unpickle::QuoteHole;
use rustc_hash::FxHashMap;
use syntax::names::Names;
use tasty::write::{Addr, Pickler, TreeBuffer};
use tasty::{Binder, SymbolId, TypeId, Universe};

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
    params: FxHashMap<Binder, Vec<String>>,
    holes: FxHashMap<TNodeId, (u32, Vec<TNodeId>)>,
    /// The package of the quote, whose members the typer refers to through the package.
    packages: Vec<SymbolId>,
    /// The definitions of the quote: their addresses, `None` until pickled (dotty's `symRefs`).
    defs: FxHashMap<SymbolId, Option<Addr>>,
    /// The references to definitions not pickled yet (dotty's `forwardSymRefs`).
    forward: FxHashMap<SymbolId, Vec<Addr>>,
}

/// The TASTy of quote `root`, `holes` its term holes, without positions (dotty's `pickleQuote`).
pub fn pickle_quote(
    cx: (&mut Universe, &Tpd, &Names),
    root: TNodeId,
    holes: &[QuoteHole],
) -> Result<Vec<u8>, Refusal> {
    pickle(cx, root, holes, None)
}

/// The TASTy of quote `root` of `source`, `holes` its term holes, with positions.
pub fn pickle_positioned_quote(
    cx: (&mut Universe, &Tpd, &Names),
    root: TNodeId,
    holes: &[QuoteHole],
    source: &QuoteSource,
) -> Result<Vec<u8>, Refusal> {
    pickle(cx, root, holes, Some(source))
}

fn pickle(
    (u, tpd, names): (&mut Universe, &Tpd, &Names),
    root: TNodeId,
    holes: &[QuoteHole],
    source: Option<&QuoteSource>,
) -> Result<Vec<u8>, Refusal> {
    let mut pickler = Pickler::default();
    let section = pickler.new_section("ASTs");
    let holes = holes.iter().map(|h| (h.node, (h.index, h.args.clone()))).collect();
    let packages = enclosing_packages(u, tpd, root);
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
        packages,
        defs: FxHashMap::default(),
        forward: FxHashMap::default(),
    };
    p.tree(root)?;
    if !p.forward.is_empty() {
        return Err("a reference to a local definition outside the quote");
    }
    p.buf.compactify();
    let bytes = std::mem::take(&mut p.buf.buf.bytes);
    p.pickler.set_section(section, bytes);
    if let Some(source) = source {
        p.positions(root, source)?;
    }
    Ok(p.pickler.assemble())
}

/// The package of the class an inlined quote `root`'s call refers to.
fn enclosing_packages(u: &Universe, tpd: &Tpd, root: TNodeId) -> Vec<SymbolId> {
    let crate::tpd::Tree::Inlined { call: Some(call), .. } = *tpd.tree(root) else { return Vec::new() };
    std::iter::successors(tpd.sym(call), |&s| u.symbol(s).owner)
        .find(|&s| u.symbol(s).kind == tasty::Kind::Package)
        .into_iter()
        .collect()
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
