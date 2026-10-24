// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The local values `quotes.reflect` defines: `Symbol.newVal`, `ValDef.apply`, `ValDef.let`.

use super::Reflect;
use crate::tpd::TNodeId;
use tasty::tags::{GIVEN, IMPLICIT, LAZY, MUTABLE, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, TypeId};

impl Reflect<'_> {
    /// A value `name` of `owner` and type `info` with the TASTy modifier bits `flags`.
    pub fn new_val(
        &mut self,
        owner: SymbolId,
        name: &str,
        info: TypeId,
        flags: u64,
    ) -> Result<SymbolId, &'static str> {
        let allowed = [LAZY, MUTABLE, SYNTHETIC, IMPLICIT, GIVEN].iter().fold(0, |b, &t| b | 1u64 << t);
        if flags & !allowed != 0 {
            return Err("a local value with flags other than Lazy, Mutable, Synthetic, Implicit and Given");
        }
        let s = self.u.define(Symbol {
            name: name.to_owned(),
            kind: Kind::Val,
            owner: None,
            flags: Flags(flags),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(s, owner);
        self.u.set_info(s, info);
        Ok(s)
    }

    /// `ValDef(s, rhs)`, of the type tree of `s`'s type.
    pub fn val_def(&mut self, s: SymbolId, rhs: Option<TNodeId>) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let info = self.u.info(s);
        let tpt = self.type_tree(info);
        Ok(self.build()?.val_def(s, tpt, rhs, span))
    }

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
        let s = self.new_val(owner, name, info, flags | 1u64 << SYNTHETIC)?;
        let vdef = self.val_def(s, Some(rhs))?;
        let span = self.span;
        Ok((vdef, self.build()?.static_ident(s, span)))
    }
}
