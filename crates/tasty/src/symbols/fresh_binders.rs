//! Binders and skolems for types made outside TASTy.

use super::{FileId, Universe};
use crate::types::{Binder, TypeId};

impl Universe {
    /// The file of the common binders of two type lambdas or polymorphic types being compared.
    pub const COMPARED_BINDERS: FileId = FileId(u32::MAX - 5);

    /// The file of the binders of `fresh_binder`.
    pub const FRESH_BINDERS: FileId = FileId(u32::MAX - 1);

    /// How many binders `fresh_binder` has made.
    pub fn binders_made(&self) -> u32 {
        self.binders
    }

    /// A binder no other type uses, for a type lambda or method type made outside TASTy.
    pub fn fresh_binder(&mut self) -> Binder {
        self.binders += 1;
        Binder(Self::FRESH_BINDERS, self.binders)
    }

    /// A skolem of type `info`, different from every other.
    pub fn fresh_skolem(&mut self, info: TypeId) -> TypeId {
        let b = self.fresh_binder();
        self.intern_type(crate::Type::Skolem(b, info))
    }
}
