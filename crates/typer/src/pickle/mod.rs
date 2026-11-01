// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/quoted/PickledQuotes.scala
//! dotty's `TreePickler` over typed trees, for the TASTy of quotes.

mod names;
mod refs;
mod trees;
mod types;

use crate::tpd::{TNodeId, Tpd};
use crate::unpickle::QuoteHole;
use rustc_hash::FxHashMap;
use syntax::names::Names;
use tasty::write::{Addr, Pickler, TreeBuffer};
use tasty::{Binder, SymbolId, TypeId, Universe};

/// Why a tree is outside the subset pickled.
pub type Refusal = &'static str;

/// What `SHAREDtype` refers back to: a type, or the `this` of a package, which dotty keeps apart
/// from the package's reference.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Shared {
    Type(TypeId),
    PackageThis(SymbolId),
}

pub struct TreePickler<'a> {
    u: &'a mut Universe,
    tpd: &'a Tpd,
    names: &'a Names,
    pickler: Pickler,
    buf: TreeBuffer,
    types: FxHashMap<Shared, Addr>,
    binders: FxHashMap<Binder, Addr>,
    holes: FxHashMap<TNodeId, (u32, Vec<TNodeId>)>,
}

/// The TASTy of quote `root`, `holes` its term holes, without positions (dotty's `pickleQuote`).
pub fn pickle_quote(
    (u, tpd, names): (&mut Universe, &Tpd, &Names),
    root: TNodeId,
    holes: &[QuoteHole],
) -> Result<Vec<u8>, Refusal> {
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
        binders: FxHashMap::default(),
        holes,
    };
    p.tree(root)?;
    p.buf.compactify();
    let bytes = std::mem::take(&mut p.buf.buf.bytes);
    p.pickler.set_section(section, bytes);
    Ok(p.pickler.assemble())
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
