// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Scanners.scala
//! Leading infix operators: an operator starting a line continues the line before.

use super::indent::IndentWidth;
use super::region::RegionKind;
use super::{Scanner, TokenData};
use crate::config::Features;
use crate::token::Token;

impl Scanner<'_> {
    /// Whether the current token is an operator continuing the expression of the line before.
    pub fn is_leading_infix_operator(&mut self, next_width: &IndentWidth, in_conditional: bool) -> bool {
        let ch = self.reader.ch;
        let cc_arrow = self.cfg.features.contains(Features::CAPTURE_CHECKING)
            && self.cur.token == Token::Identifier
            && matches!(self.name_text(), "->" | "?->")
            && ch == '{';
        if !(self.allow_leading_infix
            && self.is_operator(&self.cur)
            && (matches!(ch, ' ' | '\t' | '\r' | '\n') || cc_arrow)
            && !self.past_blank_line()
            && self.operand_follows()
            && self.region_admits_operator(next_width))
        {
            return false;
        }
        if self.cfg.source_version.migrate_to_3() {
            let (what, previous) = if in_conditional {
                ("Rest of line", "previous expression in parentheses")
            } else {
                ("Line", "expression on the previous line")
            };
            self.diags.warning(self.cur.offset, format!("{what} starts with an operator;\nit is now treated as a continuation of the {previous},\nnot as a separate statement."));
        }
        true
    }

    /// Whether an expression starts after the operator, on its line or indented on the next.
    fn operand_follows(&mut self) -> bool {
        let offset_width = self.indent_width(self.cur.offset);
        let exprs = self.can_start_expr_tokens();
        let mut la = self.lookahead_scanner();
        la.allow_leading_infix = false;
        la.next_token();
        let starts = |s: &Scanner, td: &TokenData| {
            (exprs.contains(td.token) || td.token == Token::ColonEol)
                && (!s.is_operator(td)
                    || td.name.is_some_and(|n| matches!(s.names.text(n), "-" | "+" | "~" | "!")))
        };
        starts(&la, &la.cur)
            || la.cur.token == Token::Newline
                && starts(&la, &la.next)
                && offset_width.le(&la.indent_width(la.next.offset))
    }

    fn region_admits_operator(&self, next_width: &IndentWidth) -> bool {
        let r = self.region;
        let RegionKind::Indented { width, .. } = &self.regions.get(r).kind else { return true };
        if width.le(next_width) {
            return true;
        }
        let Some(outer) = self.regions.get(r).outer else { return true };
        match &self.regions.get(outer).kind {
            RegionKind::Indented { width: ow, .. } => {
                ow.lt(next_width) && !self.regions.get(outer).other_widths.contains(next_width)
            }
            _ => self.regions.indent_width(outer).lt(next_width),
        }
    }
}
