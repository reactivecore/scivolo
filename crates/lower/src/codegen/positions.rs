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

    /// The position of `span`'s point, or `None` if it has none; for a span in another source,
    /// of its offsets in the file of the class generated.
    pub fn pos_of(&mut self, span: Span) -> Option<PosId> {
        if !span.exists() {
            return None;
        }
        let (src, point) = match span.is_in_other_source() {
            true => {
                let class = self.class.and_then(|c| self.spans.get(&c)).map_or(0, |s| s.start());
                let src = self.l.sources.locate(class).0;
                (src, src.advance_utf16(0, span.in_own_source().point() as usize))
            }
            false => self.l.sources.locate(span.point()),
        };
        let line = src.offset_to_line(point) as u32;
        let column = src.column(point) as u32;
        let file = self.str(&src.name);
        Some(PosId(self.pool.positions.intern(Pos { file, line, column })))
    }

    /// The position of `span`'s point taken as an offset in the source of class `class`, as
    /// nscplugin positions the forwarder of a member defined in another file of the unit.
    pub fn pos_in_source_of(&mut self, span: Span, class: SymbolId) -> PosId {
        if span.is_in_other_source() {
            return self.pos_or(span, self.no_pos);
        }
        let (src, point) = self.l.sources.locate(span.point());
        let units = src.text.get(..point as usize).map_or(0, |t| t.encode_utf16().count());
        self.offset_in_source_of(class, units).unwrap_or_else(|| self.pos_or(span, self.no_pos))
    }

    /// The position `units` UTF-16 units into the source of class `class` of this unit.
    pub fn offset_in_source_of(&mut self, class: SymbolId, units: usize) -> Option<PosId> {
        let at = self.spans.get(&class)?.point();
        let (base, src) = self.l.sources.files()[self.l.sources.index(at)];
        self.pos_of(Span::at(base + src.advance_utf16(0, units)))
    }

    /// The position of `span`, or `or` if it has none.
    pub fn pos_or(&mut self, span: Span, or: PosId) -> PosId {
        self.pos_of(span).unwrap_or(or)
    }
}
