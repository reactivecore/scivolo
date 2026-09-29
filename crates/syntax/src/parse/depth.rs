//! A limit on the nesting of recursively parsed constructs.

use super::Parser;
use crate::ast::NodeId;
use crate::source::Offset;

/// The deepest nesting of expressions, types, patterns, templates, packagings and XML elements.
const MAX_DEPTH: u32 = 256;

impl Parser<'_> {
    /// Run `f` one nesting level deeper; beyond `MAX_DEPTH`, give up on the source and return
    /// an error tree.
    pub fn nested(&mut self, f: impl FnOnce(&mut Self) -> NodeId) -> NodeId {
        if !self.enter_nesting() {
            let start = self.offset();
            let t = self.error_term_tree(start);
            self.give_up(start);
            return t;
        }
        let t = f(self);
        self.depth -= 1;
        t
    }

    /// Go one nesting level deeper, unless that is beyond `MAX_DEPTH`.
    pub fn enter_nesting(&mut self) -> bool {
        let ok = self.depth < MAX_DEPTH;
        if ok {
            self.depth += 1;
        }
        ok
    }

    pub fn leave_nesting(&mut self) {
        self.depth -= 1;
    }

    /// Report that nesting at `offset` is too deep, move to the end of the source and report
    /// no further errors.
    pub fn give_up(&mut self, offset: Offset) {
        self.syntax_error("nesting is too deep", offset);
        self.s.skip_to_end();
        self.gave_up = true;
    }
}
