// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The annotations an export forwarder and its parameters take from the member it forwards to
//! (dotty's `addForwarder`).

use crate::typer::Typer;
use tasty::SymbolId;
use tasty::defn::{MACRO_ANNOTATION, TAILREC};

impl Typer<'_> {
    /// Gives export forwarder `fwd` the annotations of `target`, a definition of the unit, but
    /// `@tailrec`, `@main` and macro annotations, last written first; a member of the library
    /// keeps its annotations to itself.
    pub fn forward_annotations(&mut self, fwd: SymbolId, target: SymbolId) {
        self.copy_annotations(target, fwd, true);
    }

    /// Gives parameter `to` of an export forwarder the annotations of parameter `from` of its
    /// target, a definition of the unit, last written first.
    pub fn forward_param_annotations(&mut self, from: SymbolId, to: SymbolId) {
        self.copy_annotations(from, to, false);
    }

    /// Gives `to` the annotations of `from` in reverse order, of a member only those an export
    /// forwarder takes.
    fn copy_annotations(&mut self, from: SymbolId, to: SymbolId, member: bool) {
        let mut classes = Vec::new();
        let queued: Vec<_> = self.syms.annotation_trees.iter().filter(|a| a.0 == from).copied().collect();
        for (_, annot, ctx) in queued.into_iter().rev() {
            let Some((class, _)) = self.annotation_class(annot, ctx) else { continue };
            if !member || self.is_forwarded_annotation(class) {
                classes.push(class);
                self.syms.annotation_trees.push((to, annot, ctx));
            }
        }

        let typed = self.syms.annotations.get(&from).cloned().unwrap_or_default();
        for annot in typed.into_iter().rev() {
            let Some(class) = self.u.class_symbol(self.tpd.tpe(annot)) else { continue };
            if !member || self.is_forwarded_annotation(class) {
                classes.push(class);
                self.syms.annotations.entry(to).or_default().push(annot);
                self.tpd.annotate(to, annot);
            }
        }

        let mut copied = Vec::new();
        for class in classes {
            if !copied.contains(&class) {
                copied.push(class);
                self.u.copy_annotation(from, to, class);
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
