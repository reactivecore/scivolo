// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The companion objects holding the default getters of constructors (dotty's
//! `findDefaultGetter`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::tags::{GIVEN, SYNTHETIC};
use tasty::{SymbolId, Type};

impl Typer<'_> {
    /// A reference at `at` to the companion object of the class constructor `ctor` creates in
    /// `new_tree`.
    pub fn companion_ref(
        &mut self,
        new_tree: TNodeId,
        ctor: SymbolId,
        at: syntax::ast::Span,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let class = self.u.symbol(ctor).owner.ok_or("default argument")?;
        if self.u.symbol(class).owner.is_none() {
            return Err("default argument");
        }
        let module = self.u.companion_module(class).ok_or("default argument without a companion")?;
        let new_tpe = self.tpd.tpe(new_tree);
        let new_tpe = self.u.dealias(new_tpe);
        let class_tpe = self.u.base_type(new_tpe, class).ok_or("default argument")?;
        let tycon = match *self.u.types().get(class_tpe) {
            Type::Applied(t, _) => t,
            _ => class_tpe,
        };
        let prefix = match *self.u.types().get(tycon) {
            Type::Sym(prefix, _) => prefix,
            Type::TypeRef(prefix, _) => Some(prefix),
            _ => return Err("default argument of a class of another type"),
        };
        let tpe = self.ref_type(prefix, module);
        Ok(self.ref_(prefix, module, tpe, at, ctx))
    }

    /// The companion of the given class given method `method` wraps.
    pub fn given_class_companion(&mut self, method: SymbolId) -> Option<SymbolId> {
        let info = self.u.info(method);
        let result = self.final_result(info);
        let class = self.u.class_symbol(result).filter(|_| self.u.symbol(method).flags.has(GIVEN))?;
        let flags = self.u.symbol(class).flags;
        (flags.has(GIVEN) && flags.has(SYNTHETIC)).then(|| self.u.companion_module(class)).flatten()
    }
}
