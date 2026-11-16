use super::{Refusal, TreePickler};
use crate::tpd::TNodeId;
use crate::unpickle::is_new_without_arguments;
use tasty::TypeId;

/// Why a type is not pickled before the library annotation tree it needs is read.
pub const UNREAD_ANNOTATION: Refusal = "a library annotation tree not read";

impl TreePickler<'_> {
    /// The annotation tree a library's TASTy gave annotated type `annotated`, read; none in a
    /// quote. Refuses a type several library trees gave, one whose tree is not read yet, which
    /// `pickle_file` reads before pickling again, and in a quote one whose tree has arguments.
    pub fn library_annotation(&mut self, annotated: TypeId) -> Result<Option<TNodeId>, Refusal> {
        let Some(tree) = self.u.types().annotation_tree(annotated) else { return Ok(None) };
        let tree = tree.ok_or("an annotated type of several library annotation trees")?;
        if self.file.is_none() {
            if !is_new_without_arguments(self.u.file(tree.file).0, tree.node) {
                return Err("a library annotation tree with arguments in a quote");
            }
            return Ok(None);
        }
        match self.tpd.library_annotation(tree) {
            Some(read) => read.map(Some),
            None => {
                self.unread = Some(tree);
                Err(UNREAD_ANNOTATION)
            }
        }
    }
}
