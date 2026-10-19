// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/xml/MarkupParserCommon.scala
//   compiler/src/dotty/tools/dotc/parsing/xml/MarkupParsers.scala
// MarkupParserCommon.scala: Copyright (c) 2003-2013, LAMP/EPFL.
//! Elements and their content: text, references, CDATA, comments and processing instructions.

use super::lexical::SU;
use super::{Abort, XmlResult};
use crate::ast::{NodeId, Span};
use crate::parse::Parser;

impl Parser<'_> {
    /// `<` then an end tag (not consumed, `true`), CDATA, a comment, a processing instruction or
    /// an element, added to `ts`.
    pub fn content_lt(&mut self, ts: &mut Vec<NodeId>) -> XmlResult<bool> {
        let t = match self.ch() {
            '/' => return Ok(true),
            '!' => {
                self.xml_next();
                if self.ch() == '[' { self.x_char_data()? } else { self.x_comment()? }
            }
            '?' => {
                self.xml_next();
                self.x_proc_instr()?
            }
            _ => self.element()?,
        };
        ts.push(t);
        Ok(false)
    }

    /// The content of an element up to its end tag.
    fn content(&mut self) -> XmlResult<Vec<NodeId>> {
        let mut ts = Vec::new();
        loop {
            if self.xml.embedded_block {
                ts.push(self.x_embedded_expr());
                continue;
            }
            self.xml.tmppos = Span::at(self.xml_offset());
            let p = self.xml.tmppos;
            match self.ch() {
                '<' => {
                    self.xml_next();
                    if self.content_lt(&mut ts)? {
                        return Ok(ts);
                    }
                }
                '{' if self.check_embedded_block() => ts.push(self.x_embedded_expr()),
                '&' => self.content_amp(&mut ts)?,
                SU => return Ok(ts),
                _ => {
                    let text = self.x_text()?;
                    ts.push(self.xml_text(p, &text));
                }
            }
        }
    }

    /// `{` starting a Scala block rather than `{{`; the block's `{` is skipped.
    pub fn check_embedded_block(&mut self) -> bool {
        let embedded = self.ch() == '{' && {
            self.xml_next();
            self.ch() != '{'
        };
        self.xml.embedded_block = embedded;
        embedded
    }

    /// An entity or character reference after `&`, added to `ts`.
    fn content_amp(&mut self, ts: &mut Vec<NodeId>) -> XmlResult<()> {
        self.xml_next();
        let p = self.xml.tmppos;
        let t = if self.ch() == '#' {
            self.xml_next();
            let units = self.x_char_ref()?;
            let t = self.xml_text_units(p, units);
            self.x_token(';')?;
            t
        } else {
            let name = self.x_name()?;
            self.x_token(';')?;
            self.entity_ref(p, &name)
        };
        ts.push(t);
        Ok(())
    }

    /// Character data up to `<`, `&` or an embedded block; `}}` stands for `}`.
    pub fn x_text(&mut self) -> XmlResult<String> {
        let mut text = String::new();
        while self.ch() != SU {
            if self.ch() == '}' {
                if self.char_coming_after(Self::xml_next) == '}' {
                    self.xml_next();
                } else {
                    self.xml_error("in XML content, please use '}}' to express '}'");
                    return Err(Abort::ConfusedAboutBraces);
                }
            }
            text.push(self.ch());
            self.xml_next();
            if self.check_embedded_block() || self.ch() == '<' || self.ch() == '&' {
                return Ok(text);
            }
        }
        Ok(text)
    }

    /// `Name {S Attribute} [S] ('/>' | '>' content ETag)` after `<`, one nesting level deeper.
    fn element(&mut self) -> XmlResult<NodeId> {
        self.xml_nested(Self::element_at_level)
    }

    fn element_at_level(&mut self) -> XmlResult<NodeId> {
        let start = self.xml_offset() - 1;
        let name = self.x_name()?;
        self.x_space_opt();
        let attrs = self.x_attributes()?;
        if self.ch() == '/' {
            self.x_tokens("/>")?;
            let span = Span::new(start, self.xml_offset(), start);
            return Ok(self.xml_element(span, &name, attrs, true, &[]));
        }
        self.x_token('>')?;
        if name == "xml:unparsed" {
            let text = self.x_take_until("</xml:unparsed>")?;
            let span = Span::new(start, self.xml_offset(), start);
            return Ok(self.xml_unparsed(span, &text));
        }
        self.xml.start_tags.push((start, name.clone()));
        let ts = self.content()?;
        self.x_end_tag(&name)?;
        self.xml.start_tags.pop();
        let span = Span::new(start, self.xml_offset(), start);
        Ok(if name == "xml:group" {
            self.xml_group(span, &ts)
        } else {
            self.xml_element(span, &name, attrs, false, &ts)
        })
    }

    /// `/ Name [S] >` closing `start_name`.
    pub fn x_end_tag(&mut self, start_name: &str) -> XmlResult<()> {
        self.x_token('/')?;
        if self.x_name()? != start_name {
            self.xml_error(&format!("expected closing tag of {start_name}"));
            return Err(Abort::MissingEndTag);
        }
        self.x_space_opt();
        self.x_token('>')
    }

    /// `[CDATA[ ... ]]>` after `<!`.
    fn x_char_data(&mut self) -> XmlResult<NodeId> {
        let start = self.xml_offset();
        self.x_tokens("[CDATA[")?;
        let mid = self.xml_offset();
        let text = self.x_take_until("]]>")?;
        let span = Span::new(start, self.xml_offset(), mid);
        Ok(self.xml_char_data(span, &text))
    }

    /// `-- ... -->` after `<!`.
    fn x_comment(&mut self) -> XmlResult<NodeId> {
        let start = self.xml_offset() - 2;
        self.x_tokens("--")?;
        let text = self.x_take_until("-->")?;
        let span = Span::new(start, self.xml_offset(), start);
        Ok(self.xml_comment(span, &text))
    }

    /// `Name [S] ... ?>` after `<?`.
    fn x_proc_instr(&mut self) -> XmlResult<NodeId> {
        let name = self.x_name()?;
        self.x_space_opt();
        let text = self.x_take_until("?>")?;
        let p = self.xml.tmppos;
        Ok(self.xml_proc_instr(p, &name, &text))
    }
}
