//! Inserting NEWLINE, NEWLINES, INDENT and OUTDENT before a token that starts a line.

use super::Scanner;
use super::indent::IndentWidth;
use super::region::{RegionId, RegionKind};
use crate::token::Token;
use crate::token::sets::{
    CAN_END_STAT_TOKENS, CAN_START_EXPR_TOKENS2, CAN_START_EXPR_TOKENS3, CAN_START_INDENT_TOKENS,
    CAN_START_STAT_TOKENS2, CAN_START_STAT_TOKENS3, CLOSING_REGION_TOKENS, OPEN_PARENS_TOKENS,
    STAT_CTD_TOKENS, TokenSet,
};

impl Scanner<'_> {
    pub fn indent_width(&self, offset: u32) -> IndentWidth {
        IndentWidth::at(&self.source.text, offset)
    }

    fn can_start_stat_tokens(&self) -> TokenSet {
        if self.cfg.source_version.migrate_to_3() { CAN_START_STAT_TOKENS2 } else { CAN_START_STAT_TOKENS3 }
    }

    pub fn can_start_expr_tokens(&self) -> TokenSet {
        if self.cfg.source_version.migrate_to_3() { CAN_START_EXPR_TOKENS2 } else { CAN_START_EXPR_TOKENS3 }
    }

    pub(super) fn handle_new_line(&mut self, last_token: Token) {
        let next_width = self.indent_width(self.cur.offset);
        let r = self.region;
        let (significant, separating, last_width, prefix) = match &self.regions.get(r).kind {
            RegionKind::Indented { width, prefix } => {
                let sep = width.le(&next_width) || self.regions.is_outermost(r);
                (self.indent_syntax, sep, width.clone(), *prefix)
            }
            RegionKind::InString { .. } | RegionKind::SingleLineLambda => {
                (false, false, IndentWidth::zero(), Token::Empty)
            }
            kind => {
                let braces = *kind == RegionKind::InBraces;
                self.regions.propose_known_width(r, &next_width, last_token);
                (self.indent_syntax, braces, self.regions.indent_width(r), Token::Empty)
            }
        };
        let continuing = last_width.lt(&next_width)
            && (OPEN_PARENS_TOKENS.contains(self.cur.token) || last_token == Token::Return)
            && !self.past_blank_line()
            && !self.cfg.source_version.migrate_to_3()
            && self.cfg.indent_syntax();
        if self.regions.closed_by(r) == Token::EndLambda {
            self.insert(Token::EndLambda, self.cur.line_offset.unwrap_or(self.cur.offset));
        } else if separating
            && CAN_END_STAT_TOKENS.contains(last_token)
            && self.can_start_stat_tokens().contains(self.cur.token)
            && !self.is_leading_infix_operator(&next_width, true)
            && !continuing
        {
            let t = if self.past_blank_line() { Token::Newlines } else { Token::Newline };
            self.insert(t, self.cur.line_offset.unwrap_or(self.cur.offset));
        } else if significant {
            let matchlike = matches!(prefix, Token::Match | Token::Catch);
            if next_width.lt(&last_width)
                || next_width == last_width && matchlike && self.cur.token != Token::Case
            {
                self.dedent(r, &next_width, &last_width, last_token);
            } else if last_width.lt(&next_width)
                || last_width == next_width
                    && matches!(last_token, Token::Match | Token::Catch)
                    && self.cur.token == Token::Case
            {
                if CAN_START_INDENT_TOKENS.contains(last_token) {
                    let kind = RegionKind::Indented { width: next_width.clone(), prefix: last_token };
                    self.region = self.regions.push(kind, Some(r));
                    self.insert(Token::Indent, self.cur.offset);
                } else if last_token == Token::SelfArrow {
                    self.regions.get_mut(r).known_width = Some(next_width.clone());
                }
            } else if last_width != next_width {
                self.diags.error(
                    self.cur.offset,
                    format!("Incompatible combinations of tabs and spaces in indentation prefixes.\nPrevious indent : {last_width}\nLatest indent   : {next_width}"),
                );
            }
        }
        if self.cur.token != Token::Outdent {
            let cur = self.region;
            if self.is_new_indent_width(cur, &next_width, &last_width) {
                self.regions.get_mut(cur).other_widths.push(next_width);
            }
        }
    }

    fn dedent(&mut self, r: RegionId, next_width: &IndentWidth, last_width: &IndentWidth, last_token: Token) {
        if self.regions.is_outermost(r) {
            if next_width.lt(last_width) {
                self.region = self.top_level_region(next_width.clone());
            }
            return;
        }
        let can_dedent = last_token != Token::Indent
            && !self.is_leading_infix_operator(next_width, true)
            && !STAT_CTD_TOKENS.contains(last_token)
            && !self.is_trailing_blank_line();
        if !can_dedent {
            return;
        }
        if self.regions.is_indented(r) {
            self.insert(Token::Outdent, self.cur.offset);
            let ir = self.regions.enclosing(r);
            if self.is_new_indent_width(ir, next_width, last_width) {
                if self.next.token == Token::Dot
                    && !next_width.is_close(&self.regions.indent_width(r))
                    && !next_width.is_close(&self.regions.indent_width(ir))
                {
                    self.regions.get_mut(ir).other_widths.push(next_width.clone());
                } else {
                    let RegionKind::Indented { width, .. } = &self.regions.get(ir).kind else {
                        unreachable!()
                    };
                    let message = format!(
                        "The start of this line does not match any of the previous indentation widths.\nIndentation width of current line : {next_width}\nThis falls between previous widths: {width} and {last_width}"
                    );
                    self.diags.error(self.cur.offset, message);
                }
            }
        } else if self.skipping {
            if self.regions.is_closed_by_undent_at(self.regions.enclosing(r), next_width) {
                self.insert(Token::Outdent, self.cur.offset);
            }
        } else if self.regions.get(r).kind == RegionKind::InBraces
            && !CLOSING_REGION_TOKENS.contains(self.cur.token)
        {
            self.diags.warning(self.cur.offset, "Line is indented too far to the left, or a '}' is missing");
        }
    }

    /// Whether `next_width` is an indentation width the `Indented` region `r` has not seen.
    fn is_new_indent_width(&self, r: RegionId, next_width: &IndentWidth, last_width: &IndentWidth) -> bool {
        let region = self.regions.get(r);
        matches!(&region.kind, RegionKind::Indented { width, .. } if width.lt(next_width))
            && !region.other_widths.contains(next_width)
            && next_width != last_width
    }

    /// Whether the input ends in a blank line, so no OUTDENT is due before EOF.
    fn is_trailing_blank_line(&self) -> bool {
        if self.cur.token != Token::Eof {
            return false;
        }
        let bytes = self.source.text.as_bytes();
        let Some(end) = bytes.len().checked_sub(1) else { return true };
        match bytes[..=end].iter().rposition(|&b| !matches!(b, b' ' | b'\t' | b'\r')) {
            None => true,
            Some(prev) => end > prev && matches!(bytes[prev], b'\n' | 0x0C | b'\r' | 0x1A),
        }
    }

    /// Whether a blank line lies between the last token and the current one.
    pub(super) fn past_blank_line(&self) -> bool {
        let bytes = self.source.text.as_bytes();
        let mut blank = false;
        for &b in &bytes[self.cur.last_offset as usize..(self.cur.offset as usize).min(bytes.len())] {
            if b == b'\n' || b == 0x0C {
                if blank {
                    return true;
                }
                blank = true;
            } else {
                blank = blank && b <= b' ';
            }
        }
        false
    }
}
