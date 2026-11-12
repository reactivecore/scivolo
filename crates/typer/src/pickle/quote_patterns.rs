// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Quote patterns (the `QuotePattern` and `SplicePattern` cases of dotty's `pickleTree`).

use super::{Refusal, TreePickler};
use crate::tpd::{List, TNodeId};
use tasty::tags::{EXPLICITtpt, QUOTEPATTERN, SPLICEPATTERN};

impl TreePickler<'_> {
    /// Pickles quote pattern `id`, `'{ body }` or `'[ body ]` with `quotes` and type bindings
    /// `bindings`.
    pub fn quote_pattern(
        &mut self,
        id: TNodeId,
        bindings: List,
        body: TNodeId,
        quotes: TNodeId,
    ) -> Result<(), Refusal> {
        let tpe = self.tpd.get(id).tpe.ok_or("a quote pattern without a type")?;
        let bindings = self.tpd.items(bindings).to_vec();
        self.with_length(QUOTEPATTERN, &mut |p| {
            if p.tpd.is_type_tree(body) {
                p.byte(EXPLICITtpt);
            }
            p.tree(body)?;
            p.tree(quotes)?;
            p.ty(tpe, false)?;
            bindings.iter().try_for_each(|&b| p.tree(b))
        })
    }

    /// Pickles splice pattern `id`, `$body` or `$body[typeargs](args)`.
    pub fn splice_pattern(
        &mut self,
        id: TNodeId,
        body: TNodeId,
        typeargs: List,
        args: List,
    ) -> Result<(), Refusal> {
        let tpe = self.tpd.get(id).tpe.ok_or("a splice pattern without a type")?;
        let (typeargs, args) = (self.tpd.items(typeargs).to_vec(), self.tpd.items(args).to_vec());
        self.with_length(SPLICEPATTERN, &mut |p| {
            p.tree(body)?;
            p.ty(tpe, false)?;
            for &t in &typeargs {
                p.byte(EXPLICITtpt);
                p.tree(t)?;
            }
            args.iter().try_for_each(|&a| p.tree(a))
        })
    }
}
