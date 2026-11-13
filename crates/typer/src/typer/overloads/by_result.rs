// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Choosing among overloaded alternatives by the expected result type (dotty's `adaptByResult`).

use super::{AMBIGUOUS, NO_APPLICABLE, Resolution, UNDECIDED_RESULT, Unchosen};
use crate::typer::{Pt, Typer};
use tasty::SymbolId;

impl Resolution {
    /// Checks whether the result of `alt` conforms to the expected result type of `proto`, if
    /// it has one; `None` if undecided.
    pub fn result_fits(self, t: &mut Typer, alt: SymbolId, proto: usize) -> Option<bool> {
        let (Pt::Type(pt), ctx) = (t.protos[proto].pt, t.protos[proto].ctx) else { return Some(true) };
        t.result_conforms(alt, self.prefix, pt, (true, ctx))
    }

    /// Gives `chosen`, or if its result does not conform to the expected result type of
    /// `proto`, the one alternative of `alts` whose result does, or the one resolution picks
    /// among them (dotty's `adaptByResult`); `chosen` if its result is undecided, refused if
    /// another's is.
    pub fn adapted_by_result(
        self,
        t: &mut Typer,
        chosen: SymbolId,
        alts: &[SymbolId],
        proto: usize,
    ) -> Result<SymbolId, Unchosen> {
        if self.result_fits(t, chosen, proto) != Some(false) {
            return Ok(chosen);
        }

        let mut conforming = Vec::new();
        for &a in alts.iter().filter(|&&a| a != chosen) {
            match self.result_fits(t, a, proto) {
                Some(true) => conforming.push(a),
                Some(false) => {}
                None => return Err(UNDECIDED_RESULT.into()),
            }
        }

        Ok(match conforming[..] {
            [] => chosen,
            [alt] => alt,
            _ => match self.resolve_overloaded(t, &conforming, proto) {
                Ok(alt) => alt,
                Err(e) if [NO_APPLICABLE, AMBIGUOUS].contains(&e.what) => chosen,
                Err(e) => return Err(e),
            },
        })
    }
}
