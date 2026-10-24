// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MacroAnnotations.scala
//! Macro annotations: the definitions of a statement list that an annotation extending
//! `scala.annotation.MacroAnnotation` is on.

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::OBJECT;
use tasty::{SymbolId, Type};

const MACRO_ANNOTATION: &str = "scala.annotation.MacroAnnotation";

impl Inlining<'_> {
    /// Refuses the statements of `t`, a template, block or package, if a macro annotation is on
    /// one of their definitions.
    pub fn check_macro_annotations(&mut self, t: TNodeId) -> Result<(), Unsupported> {
        let stats = match *self.tpd.tree(t) {
            T::Template { body: stats, .. } | T::Block { stats, .. } | T::PackageDef { stats, .. } => stats,
            _ => return Ok(()),
        };
        for stat in self.tpd.items(stats).to_vec() {
            if !self.macro_annotations(stat).is_empty() {
                return Err((self.tpd.span(stat), "a macro annotation"));
            }
        }
        Ok(())
    }

    /// The macro annotation classes on definition `def`, in order; none on a module value, whose
    /// class carries them (dotty's `hasMacroAnnotation`).
    fn macro_annotations(&mut self, def: TNodeId) -> Vec<SymbolId> {
        if !matches!(self.tpd.tree(def), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. }) {
            return Vec::new();
        }
        let Some(s) = self.tpd.sym(def) else { return Vec::new() };
        if self.u.symbol(s).kind.is_term() && self.u.symbol(s).flags.has(OBJECT) {
            return Vec::new();
        }
        let classes = self.u.annotations(s).to_vec();
        if classes.is_empty() {
            return Vec::new();
        }
        let base = self.u.class_ref(MACRO_ANNOTATION);
        let Type::Sym(_, base) = *self.u.types().get(base) else { return Vec::new() };
        classes.into_iter().filter(|&c| self.u.derives_from(c, base)).collect()
    }
}
