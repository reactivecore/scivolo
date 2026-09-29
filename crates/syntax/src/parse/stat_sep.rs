//! The separators and end of a statement sequence, and the scope of its placeholders and
//! language imports.

use super::Parser;
use crate::ast::NodeId;
use crate::config::Features;
use crate::token::Token;
use crate::token::sets::{CAN_START_EXPR_TOKENS3, MUST_START_STAT_TOKENS};

impl Parser<'_> {
    pub fn is_stat_seq_end(&self) -> bool {
        self.s.cur.is_nested_end() || matches!(self.tok(), Token::Eof | Token::RParen)
    }

    /// Skip statement separators and end markers after `stats`; false at the end of the
    /// sequence.
    pub fn stat_sep_or_end(
        &mut self,
        stats: &mut [NodeId],
        no_prev_stat: bool,
        what: &str,
        alt_end: Token,
    ) -> bool {
        self.s.observe_outdented();
        let (mut sep_seen, mut end_seen) = (false, false);
        loop {
            if self.s.cur.is_stat_sep() {
                self.next();
                sep_seen = true;
            } else if self.tok() == Token::End {
                if end_seen {
                    let o = self.offset();
                    self.syntax_error("duplicate end marker", o);
                }
                self.check_end_marker(stats);
                end_seen = true;
            } else if self.is_stat_seq_end() || self.tok() == alt_end {
                return false;
            } else if sep_seen || end_seen {
                return true;
            } else {
                let found = self.tok();
                let stat_follows = MUST_START_STAT_TOKENS.contains(found);
                let msg = if !no_prev_stat {
                    let extra = if self.s.is_operator(&self.s.cur)
                        && CAN_START_EXPR_TOKENS3.contains(self.s.lookahead().token)
                    {
                        " - did you intend to apply an infix operator? If so, wrap the right operand in parentheses or in a block."
                    } else {
                        ""
                    };
                    format!("end of {what} expected but {} found{extra}", found.display())
                } else if stat_follows {
                    "this kind of statement is not allowed here".to_string()
                } else {
                    let addendum =
                        if self.is_modifier() { ": this modifier is not allowed here" } else { "" };
                    format!("Illegal start of {what}{addendum}")
                };
                let o = self.offset();
                self.syntax_error(msg, o);
                if stat_follows {
                    return false;
                }
                self.next();
                self.skip();
                return true;
            }
        }
    }

    /// Run `f`, reporting a `_` it leaves unbound, and scope language imports to it.
    pub fn check_no_escaping_placeholders<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = std::mem::take(&mut self.placeholder_params);
        let mut features = self.s.cfg.features;
        let r = f(self);
        if let Some(&vd) = self.placeholder_params.last() {
            let span = self.ast.span(vd);
            self.syntax_error_span("Unbound placeholder parameter; incorrect use of _", span);
        }
        self.placeholder_params = saved;
        for unit_level in [Features::CAPTURE_CHECKING, Features::PURE_FUNCTIONS] {
            if self.s.cfg.features.contains(unit_level) {
                features.insert(unit_level);
            }
        }
        self.s.cfg.features = features;
        r
    }
}
