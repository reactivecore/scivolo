// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The annotations an export forwarder takes from the member it forwards to (dotty's
//! `addForwarder`).

use crate::typer::Typer;
use tasty::SymbolId;
use tasty::defn::{MACRO_ANNOTATION, TAILREC};

impl Typer<'_> {
    /// Gives export forwarder `fwd` the annotations of `target`, a definition of the unit, but
    /// `@tailrec`, `@main` and macro annotations, in order; a member of the library keeps its
    /// annotations to itself.
    pub fn forward_annotations(&mut self, fwd: SymbolId, target: SymbolId) {
        let queued: Vec<_> = self.syms.annotation_trees.iter().filter(|a| a.0 == target).copied().collect();
        for (_, annot, ctx) in queued {
            let Some((class, _)) = self.annotation_class(annot, ctx) else { continue };
            if self.is_forwarded_annotation(class) {
                self.u.copy_annotation(target, fwd, class);
                self.syms.annotation_trees.push((fwd, annot, ctx));
            }
        }

        let typed = self.syms.annotations.get(&target).cloned().unwrap_or_default();
        for annot in typed {
            let Some(class) = self.u.class_symbol(self.tpd.tpe(annot)) else { continue };
            if self.is_forwarded_annotation(class) {
                self.u.copy_annotation(target, fwd, class);
                self.syms.annotations.entry(fwd).or_default().push(annot);
                self.tpd.annotate(fwd, annot);
            }
        }
    }

    /// Whether an export forwarder takes an annotation of class `class` from its target.
    fn is_forwarded_annotation(&mut self, class: SymbolId) -> bool {
        let macro_base = self.u.class_ref(MACRO_ANNOTATION);
        let is_macro = self.u.sym_of(macro_base).is_some_and(|b| self.u.derives_from(class, b));
        self.u.full_name(class) != TAILREC && self.defn.main_annot(self.u) != Ok(class) && !is_macro
    }
}
