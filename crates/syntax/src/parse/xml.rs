//! XML literals, not supported: reported as errors.

use super::Parser;
use crate::ast::NodeId;

impl Parser<'_> {
    pub fn xml_literal(&mut self) -> NodeId {
        let start = self.offset();
        self.syntax_error("XML literals are not supported", start);
        self.next();
        self.error_term_tree(start)
    }

    pub fn xml_literal_pattern(&mut self) -> NodeId {
        self.xml_literal()
    }
}
