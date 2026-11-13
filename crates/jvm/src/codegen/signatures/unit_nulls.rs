//! Generic signatures built comparing types with the unit's explicit nulls.

use super::Member;
use crate::codegen::Gen;
use tasty::{NullMode, SymbolId, TypeId, Universe};

impl Gen<'_, '_> {
    /// Builds the generic signature of `m` of type `info`, descriptor `desc` and thrown exception
    /// types `thrown` as `signatures::member`.
    pub fn member_signature(
        &mut self,
        m: Member,
        info: TypeId,
        desc: &str,
        thrown: &[TypeId],
    ) -> Option<String> {
        self.with_unit_nulls(|u| super::member(u, m, info, desc, thrown))
    }

    /// Builds the generic signature of class `c` within top-level class `top` as
    /// `signatures::class`.
    pub fn class_signature(&mut self, c: SymbolId, top: SymbolId) -> String {
        self.with_unit_nulls(|u| super::class(u, c, top))
    }

    fn with_unit_nulls<T>(&mut self, f: impl FnOnce(&mut Universe) -> T) -> T {
        let mode = NullMode { explicit: self.l.explicit_nulls(), ..self.l.u.null_mode() };
        let outer = self.l.u.set_null_mode(mode);
        let done = f(self.l.u);
        self.l.u.set_null_mode(outer);
        done
    }
}
