// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The annotations an export forwarder takes from the member it forwards to (dotty's
//! `addForwarder`).

use crate::tpd::TNodeId;
use crate::typer::Typer;
use tasty::SymbolId;
use tasty::defn::{MACRO_ANNOTATION, TAILREC};

impl Typer<'_> {
    /// Gives export forwarder `fwd` the annotations of `target`, but `@tailrec`, `@main` and
    /// macro annotations, in order: those of a definition of the unit, or those read from the
    /// TASTy of a member of the library, none where that read is refused.
    pub fn forward_annotations(&mut self, fwd: SymbolId, target: SymbolId) {
        if self.u.symbol(target).tree.is_some() {
            let read = crate::unpickle::annotations(self.u, &mut self.tpd, self.names, target);
            for annot in read.unwrap_or_default() {
                self.tpd.mark_read_from(annot, target);
                self.forward_typed(fwd, target, annot);
            }
            return;
        }

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
            self.forward_typed(fwd, target, annot);
        }
    }

    /// Gives export forwarder `fwd` typed annotation `annot` of `target` if it takes its class.
    fn forward_typed(&mut self, fwd: SymbolId, target: SymbolId, annot: TNodeId) {
        let Some(class) = self.u.class_symbol(self.tpd.tpe(annot)) else { return };
        if self.is_forwarded_annotation(class) {
            self.u.copy_annotation(target, fwd, class);
            self.syms.annotations.entry(fwd).or_default().push(annot);
            self.tpd.annotate(fwd, annot);
        }
    }

    /// Whether an export forwarder takes an annotation of class `class` from its target.
    fn is_forwarded_annotation(&mut self, class: SymbolId) -> bool {
        let macro_base = self.u.class_ref(MACRO_ANNOTATION);
        let is_macro = self.u.sym_of(macro_base).is_some_and(|b| self.u.derives_from(class, b));
        self.u.full_name(class) != TAILREC && self.defn.main_annot(self.u) != Ok(class) && !is_macro
    }
}
