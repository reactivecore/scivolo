// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The expected types of the patterns of sequence extractors `C(p1, ..., xs*)` (dotty's
//! `isProductSeqMatch`, `isNonEmptyTupleSeqMatch`).

use super::super::Typer;
use super::super::extractor_members::{NON_EMPTY_TUPLE, product_selectors, seq_elem_type};
use super::{SeqArgs, UnapplyArgs};
use crate::defn::PRODUCT;
use tasty::TypeId;

impl UnapplyArgs {
    /// Determines the types of the patterns against `tpe` as a sequence, or as a product or a
    /// generic tuple whose last element is a sequence, as `(Int, Seq[String])`.
    pub fn seq_match(&self, t: &mut Typer<'_>, tpe: TypeId) -> Result<Option<SeqArgs>, &'static str> {
        if let Some(elem) = seq_elem_type(t, tpe) {
            return Ok(Some(SeqArgs { types: vec![elem; self.n], leading: 0 }));
        }
        if !t.derives_from_named(tpe, PRODUCT) {
            return Ok(None);
        }
        let selectors = product_selectors(t, tpe)?;
        if let Some(args) = self.seq_after(t, &selectors) {
            return Ok(Some(args));
        }
        if !t.derives_from_named(tpe, NON_EMPTY_TUPLE) {
            return Ok(None);
        }
        let widened = t.u.widen(tpe);
        let elems = t.tuple_elements_within(widened, usize::MAX).unwrap_or_default();
        Ok(self.seq_after(t, &elems))
    }

    /// The types of the patterns against `selectors`, the last of them a sequence.
    fn seq_after(&self, t: &mut Typer<'_>, selectors: &[TypeId]) -> Option<SeqArgs> {
        let (&last, leading) = selectors.split_last()?;
        let elem = seq_elem_type(t, last).filter(|_| selectors.len() <= self.n + 1)?;
        let types = (0..self.n).map(|i| leading.get(i).copied().unwrap_or(elem)).collect();
        Some(SeqArgs { types, leading: leading.len() })
    }
}
