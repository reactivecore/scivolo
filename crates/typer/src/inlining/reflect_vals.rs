// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The local values `quotes.reflect` refers to and defines: `Ident.apply`, `ValDef.let`.

use super::Reflect;
use crate::tpd::TNodeId;
use tasty::tags::SYNTHETIC;
use tasty::{Kind, SymbolId};

impl Reflect<'_> {
    /// An identifier of local value `s`.
    pub fn local_ident(&mut self, s: SymbolId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        Ok(self.build()?.static_ident(s, span))
    }

    /// The definition of a synthetic value `name` of `owner` with right-hand side `rhs` and the
    /// TASTy modifier bits `flags` (dotty's `SyntheticValDef`), and a reference to it.
    pub fn let_(
        &mut self,
        owner: SymbolId,
        name: &str,
        rhs: TNodeId,
        flags: u64,
    ) -> Result<(TNodeId, TNodeId), &'static str> {
        let info = self.tpd.get(rhs).tpe.ok_or("ValDef.let of a term without a type")?;
        let info = self.u.widen(info);
        let s = self.new_term((owner, name, Kind::Val), info, flags | 1u64 << SYNTHETIC)?;
        let vdef = self.val_def(s, Some(rhs))?;
        let span = self.span;
        Ok((vdef, self.build()?.static_ident(s, span)))
    }
}
