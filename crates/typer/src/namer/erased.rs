// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Values made erased by their type: those of a class deriving from `scala.compiletime.Erased`
//! (dotty's `typedValDef`, `checkErasedOK`).

use crate::typer::Typer;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// Whether `t` is a type of a class deriving from `scala.compiletime.Erased`.
    pub fn derives_erased_class(&mut self, t: TypeId) -> bool {
        let erased = self.resolve_full_name("scala.compiletime.Erased", true);
        erased.zip(self.u.class_symbol(t)).is_some_and(|(e, c)| self.u.derives_from(c, e))
    }

    /// Flags value or parameter `s` of definition `tree` erased if its type `info` derives from
    /// `scala.compiletime.Erased`; a given loses its implied laziness.
    pub fn mark_erased_by_type(&mut self, s: SymbolId, tree: NodeId, info: TypeId) {
        let Some(&flags) = self.syms.flags.get(&s) else { return };
        if flags.is(f::MODULE) || flags.is(f::ERASED) || !self.derives_erased_class(info) {
            return;
        }
        let explicit_lazy = match *self.ast.tree(tree) {
            Tree::ValDef { mods, .. } => {
                self.ast.modifiers(mods).mods.iter().any(|m| m.kind.flags().is(f::LAZY))
            }
            _ => false,
        };
        let flags = if flags.is(f::GIVEN) && !explicit_lazy { flags.and_not(f::LAZY) } else { flags };
        if flags.is_one_of(f::MUTABLE.or(f::LAZY)) {
            self.unsupported(tree, "lazy or mutable value of an erased class");
            return;
        }
        self.syms.set_flags(self.u, s, flags.or(f::ERASED));
    }
}
