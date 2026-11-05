// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! Scala 2 macro definitions, `def f: T = macro ???` (dotty's `typedScala2MacroBody` and
//! PostTyper's check for a Scala 3 implementation).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::tags::{INLINE, MACRO};
use tasty::{SymbolId, TypeId};

/// The Scala 2 macros the library may define without a Scala 3 implementation.
const WITHOUT_IMPLEMENTATION: [&str; 3] =
    ["scala.StringContext.s", "scala.StringContext.raw", "scala.StringContext.f"];

impl Typer<'_> {
    /// Right-hand side `rhs` of method `sym` defined by `def` typed against its result type
    /// `result`, a Scala 2 macro's against `Any`.
    pub fn typed_def_rhs(
        &mut self,
        (def, sym): (NodeId, SymbolId),
        rhs: NodeId,
        result: TypeId,
        ctx: Ctx,
    ) -> TNodeId {
        let flags = self.u.symbol(sym).flags;
        if !flags.has(MACRO) || flags.has(INLINE) {
            return self.typed(rhs, Pt::Type(result), ctx);
        }
        let intrinsic = matches!(*self.ast.tree(rhs), Tree::Ident { name } if self.text(name.name) == "???");
        if !intrinsic {
            return self.unsupported(rhs, "Scala 2 macro implementation");
        }
        if !self.has_scala3_implementation(sym) {
            let span = self.ast.span(def);
            self.error_at(span, "No Scala 3 implementation found for this Scala 2 macro.");
        }
        self.typed(rhs, Pt::Any, ctx)
    }

    /// Whether Scala 2 macro `sym` is one of the library's or its owner declares a method of its
    /// name and signature that is not a Scala 2 macro.
    fn has_scala3_implementation(&mut self, sym: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(sym).owner else { return false };
        if WITHOUT_IMPLEMENTATION.contains(&self.u.full_name(sym).as_str())
            || self.u.full_name(owner) == "scala.reflect.package$"
        {
            return true;
        }
        if !self.u.is_class(owner) {
            return false;
        }
        let name = self.u.symbol(sym).name.clone();
        let site = self.u.this_type(owner);
        let signature = self.u.signature(site, sym, false);
        let siblings = self.u.decl(owner, &name).unwrap_or_default();
        siblings.iter().for_each(|&p| self.complete(p));
        siblings.into_iter().any(|p| {
            let f = self.u.symbol(p).flags;
            p != sym && !(f.has(MACRO) && !f.has(INLINE)) && self.u.signature(site, p, false) == signature
        })
    }
}
