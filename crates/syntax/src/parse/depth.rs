//! A limit on the nesting of recursively parsed constructs.

use super::Parser;
use crate::ast::NodeId;
use crate::token::Token;

/// The deepest nesting of expressions, types, patterns, templates and packagings.
const MAX_DEPTH: u32 = 256;

impl Parser<'_> {
    /// Run `f` one nesting level deeper; beyond `MAX_DEPTH`, report an error, skip the rest of
    /// the source and return an error tree.
    pub fn nested(&mut self, f: impl FnOnce(&mut Self) -> NodeId) -> NodeId {
        if self.depth >= MAX_DEPTH {
            let start = self.offset();
            self.syntax_error("nesting is too deep", start);
            while self.tok() != Token::Eof {
                self.next();
            }
            self.last_error_offset = Some(self.offset());
            return self.error_term_tree(start);
        }
        self.depth += 1;
        let t = f(self);
        self.depth -= 1;
        t
    }
}
