// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala

mod seq;

use super::Typer;
use super::extractor_members::{BOOLEAN, NON_EMPTY_TUPLE, PRODUCT, SEQ, get_match, product_selectors};
use tasty::TypeId;

/// The expected types of the argument patterns of one extractor (dotty's `UnapplyArgs`): for
/// `unapply`, the product selectors of its result, as for `case class C(a: Int, b: String)`,
/// the `get` of an `isEmpty`/`get` result, or those of the `get`'s product selectors, none for a
/// `Boolean` result, the elements of a generic tuple; for `unapplySeq`, the element type of a
/// sequence, after the product selectors before a last sequence one. Refuses other result types
/// and a number of patterns that matches none of these.
pub struct UnapplyArgs {
    /// The result type of the extractor applied to the scrutinee.
    pub result: TypeId,
    /// The number of patterns.
    pub n: usize,
}

/// The types of the patterns of a sequence extractor.
pub struct SeqArgs {
    /// The expected type of each pattern.
    pub types: Vec<TypeId>,
    /// How many of them are those of product selectors before the sequence.
    pub leading: usize,
}

impl UnapplyArgs {
    /// Determines the expected types of the patterns of an `unapply` (dotty's
    /// `UnapplyArgs.argTypes`).
    pub fn arg_types(&self, t: &mut Typer<'_>) -> Result<Vec<TypeId>, &'static str> {
        let n = self.n;
        let result = t.strip_named_tuple(self.result);
        let types =
            if n > 0 && t.derives_from_named(result, PRODUCT) && product_selectors(t, result)?.len() == n {
                product_selectors(t, result)?
            } else if let Some(get) = get_match(t, result)? {
                let sels = if n > 1 && !t.derives_from_named(get, SEQ) {
                    let stripped = t.strip_named_tuple(get);
                    product_selectors(t, stripped)?
                } else {
                    Vec::new()
                };
                if sels.len() == n { sels } else { vec![get] }
            } else if t.derives_from_named(result, BOOLEAN) {
                Vec::new()
            } else if t.derives_from_named(result, NON_EMPTY_TUPLE) {
                let widened = t.u.widen(result);
                t.tuple_elements_within(widened, usize::MAX).unwrap_or_default()
            } else {
                return Err("extractor result type");
            };

        if types.len() != n {
            return Err("wrong number of patterns");
        }
        Ok(types)
    }

    /// Determines the expected types of the patterns of an `unapplySeq`, against its result or
    /// the result's `get` (dotty's `UnapplyArgs.argTypes` for `unapplySeq`).
    pub fn seq_arg_types(&self, t: &mut Typer<'_>) -> Result<SeqArgs, &'static str> {
        if let Some(args) = self.seq_match(t, self.result)? {
            return Ok(args);
        }
        let get = get_match(t, self.result)?.ok_or("sequence extractor result type")?;
        self.seq_match(t, get)?.ok_or("sequence extractor result type")
    }
}
