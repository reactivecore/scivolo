// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala

use crate::typer::Typer;
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    /// The type of object `sym`'s value, its class through the owner, without resolving the
    /// class's name (dotty's `moduleValSig`); `None` for another symbol.
    pub fn module_val_info(&mut self, sym: SymbolId) -> Option<TypeId> {
        let s = self.u.symbol(sym);
        if s.kind != Kind::Val || !s.flags.has(tasty::tags::OBJECT) {
            return None;
        }
        let owner = s.owner?;
        let class = self.u.module_class(sym)?;
        self.complete(class);
        let prefix = (!self.u.symbol(owner).kind.is_term()).then(|| crate::defn::owner_prefix(self.u, owner));
        Some(crate::defn::ref_type(self.u, prefix, class))
    }
}
