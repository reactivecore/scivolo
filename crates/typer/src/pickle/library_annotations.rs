use super::{Refusal, TreePickler};
use crate::tpd::TNodeId;
use tasty::TypeId;

/// Why a type is not pickled before the library annotation tree it needs is read.
pub const UNREAD_ANNOTATION: Refusal = "a library annotation tree not read";

impl TreePickler<'_> {
    /// The annotation tree a library's TASTy gave annotated type `annotated`, read. Refuses a type
    /// whose tree is not read yet, which `pickle_file` and `pickle_positioned_quote` read before
    /// pickling again.
    pub fn library_annotation(&mut self, annotated: TypeId) -> Result<Option<TNodeId>, Refusal> {
        let Some(tree) = self.u.types().annotation_tree(annotated) else { return Ok(None) };
        match self.tpd.library_annotation(tree) {
            Some(read) => read.map(Some),
            None => {
                self.unread = Some(tree);
                Err(UNREAD_ANNOTATION)
            }
        }
    }
}
