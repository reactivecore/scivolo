//! Error recovery and skipping parenthesized token sequences.

use super::Scanner;
use super::region::RegionKind;
use crate::token::Token;
use crate::token::sets::CLOSING_PARENS;

impl Scanner<'_> {
    pub fn drop_until(&mut self, matches: impl Fn(&RegionKind) -> bool) {
        while !matches(&self.regions.get(self.region).kind) && !self.regions.is_outermost(self.region) {
            self.region = self.regions.enclosing(self.region);
        }
    }

    /// The known indentation width of the innermost region that has one.
    fn last_known_indent_width(&self) -> super::IndentWidth {
        let r = self.regions.chain(self.region).find(|&r| self.regions.get(r).known_width.is_some());
        r.map_or_else(super::IndentWidth::zero, |r| self.regions.indent_width(r))
    }

    /// Skip tokens after an error up to a safe point: a statement separator, a closing
    /// parenthesis or comma of an enclosing region, or a dedent.
    pub fn skip(&mut self) {
        let last_region = self.region;
        self.skipping = true;
        let mut no_progress = 0;
        let mut prev_offset = self.cur.offset;
        while !self.at_skip_stop(last_region) && no_progress < 3 {
            self.next_token();
            if self.cur.offset <= prev_offset {
                no_progress += 1;
            } else {
                prev_offset = self.cur.offset;
                no_progress = 0;
            }
        }
        if self.cur.token == Token::Outdent {
            self.drop_until(|k| matches!(k, RegionKind::Indented { .. }));
        }
        self.skipping = false;
    }

    fn at_skip_stop(&self, last_region: super::RegionId) -> bool {
        let t = self.cur.token;
        t == Token::Eof
            || self.region == last_region
                && (self.cur.is_stat_sep()
                    || CLOSING_PARENS.contains(t)
                        && self.regions.chain(last_region).any(|r| self.regions.closed_by(r) == t)
                    || t == Token::Comma
                        && self.regions.chain(last_region).any(|r| self.regions.get(r).commas_expected)
                    || t == Token::Outdent
                        && self.indent_width(self.cur.offset).lt(&self.last_known_indent_width()))
    }

    /// Skip a parenthesized sequence starting at the current `(` or `[`.
    pub fn skip_parens(&mut self, multiple: bool) {
        let opening = self.cur.token;
        self.next_token();
        while self.cur.token != Token::Eof && self.cur.token != opening.closer() {
            if self.cur.token == opening && multiple {
                self.skip_parens(true);
            } else {
                self.next_token();
            }
        }
        self.next_token();
    }
}
