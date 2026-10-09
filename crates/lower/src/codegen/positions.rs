//! The source positions of what is generated.

use super::Gen;
use nir::{Pos, PosId};
use syntax::ast::Span;
use tasty::SymbolId;

impl Gen<'_, '_> {
    /// The position of class or method `s` of this unit, else of class `or`, else none.
    pub fn sym_pos(&mut self, s: SymbolId, or: SymbolId) -> PosId {
        let span = self.spans.get(&s).or_else(|| self.spans.get(&or)).copied().unwrap_or(Span::NONE);
        self.pos_or(span, self.no_pos)
    }

    /// The position of `span`'s point, or `None` if it has none.
    pub fn pos_of(&mut self, span: Span) -> Option<PosId> {
        if !span.exists() {
            return None;
        }
        let src = self.l.source;
        let line = src.offset_to_line(span.point()) as u32;
        let column = src.column(span.point()) as u32;
        Some(PosId(self.pool.positions.intern(Pos { file: self.file, line, column })))
    }

    /// The position of `span`, or `or` if it has none.
    pub fn pos_or(&mut self, span: Span, or: PosId) -> PosId {
        self.pos_of(span).unwrap_or(or)
    }
}
