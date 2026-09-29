use super::lexical::SU;
use super::{Abort, XmlResult};
use crate::ast::{NodeId, Span};
use crate::parse::Parser;

impl Parser<'_> {
    /// `Name [S] ('/>' | '>' {pattern | '{' patterns '}' | text} ETag)` after `<`, one nesting
    /// level deeper.
    pub fn x_pattern(&mut self) -> XmlResult<NodeId> {
        self.xml_nested(Self::x_pattern_at_level)
    }

    fn x_pattern_at_level(&mut self) -> XmlResult<NodeId> {
        let start = self.xml_offset() - 1;
        let name = self.x_name()?;
        self.xml.start_tags.push((start, name.clone()));
        self.x_space_opt();
        let mut ts = Vec::new();
        let empty = self.ch() == '/';
        if empty {
            self.xml_next();
        }
        self.x_token('>')?;
        if !empty {
            while self.pattern_content(&mut ts)? {}
            self.x_end_tag(&name)?;
            self.xml.start_tags.pop();
        }
        let span = Span::new(start, self.xml_offset(), start);
        Ok(self.make_xml_pat(span, &name, &ts))
    }

    /// One part of a pattern's content, added to `ts`; `false` at the end tag.
    fn pattern_content(&mut self, ts: &mut Vec<NodeId>) -> XmlResult<bool> {
        let start = self.xml_offset();
        if self.xml.embedded_block {
            ts.extend(self.x_scala_patterns());
            return Ok(true);
        }
        match self.ch() {
            '<' => {
                self.xml_next();
                if self.ch() == '/' {
                    return Ok(false);
                }
                ts.push(self.x_pattern()?);
            }
            '{' => {
                while self.ch() == '{' {
                    self.xml_next();
                    ts.extend(self.x_scala_patterns());
                }
            }
            SU => return Err(Abort::Truncated),
            _ => {
                let span = Span::new(start, self.xml_offset(), start);
                let text = self.x_text()?;
                ts.push(self.xml_text(span, &text));
            }
        }
        Ok(true)
    }
}
