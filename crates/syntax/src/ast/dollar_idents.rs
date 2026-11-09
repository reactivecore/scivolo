//! The identifiers with a `$` that the source spells out, by offset.

use super::Ast;
use crate::names::Name;
use crate::source::Offset;

impl Ast {
    pub fn note_dollar_ident(&mut self, at: Offset, name: Name) {
        self.dollar_idents.insert(at, name);
    }

    /// The identifier with a `$` written at `at`.
    pub fn dollar_ident_at(&self, at: Offset) -> Option<Name> {
        self.dollar_idents.get(&at).copied()
    }
}
