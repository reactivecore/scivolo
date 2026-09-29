//! Characters, names, whitespace and errors of XML literals (dotty's `MarkupParserCommon`).

use super::{Abort, XmlResult};
use crate::names::surrogate_char;
use crate::parse::Parser;
use crate::source::Offset;
use unicode_general_category::{GeneralCategory as G, get_general_category};

/// The character the reader returns at the end of the text.
pub const SU: char = '\u{1A}';

pub fn is_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | ' ')
}

/// Whether `c` can continue an XML name (`scala.xml.Utility.isNameChar`), for a UTF-16 unit.
pub fn is_name_char(c: char) -> bool {
    is_name_start(c)
        || (c as u32) <= 0xFFFF
            && matches!(
                get_general_category(c),
                G::SpacingMark | G::EnclosingMark | G::NonspacingMark | G::ModifierLetter | G::DecimalNumber
            )
        || matches!(c, '.' | '-' | ':')
}

pub use crate::scan::is_xml_name_start as is_name_start;

impl Parser<'_> {
    /// The current UTF-16 unit, a supplementary character's surrogates as `surrogate_char`s.
    pub fn ch(&self) -> char {
        let c = self.s.xml_ch();
        if (c as u32) <= 0xFFFF {
            return c;
        }
        let units = c.encode_utf16(&mut [0; 2]).to_vec();
        surrogate_char(units[self.xml.low_half as usize])
    }

    pub fn xml_next(&mut self) {
        if (self.s.xml_ch() as u32) > 0xFFFF && !self.xml.low_half {
            self.xml.low_half = true;
        } else {
            self.xml.low_half = false;
            self.s.xml_next_char();
        }
    }

    /// The offset of the current unit; a low surrogate is at the end of its character.
    pub fn xml_offset(&self) -> Offset {
        if self.xml.low_half { self.s.xml_offset() + 4 } else { self.s.xml_offset() }
    }

    /// Report `msg` at the current character and skip it (dotty's `reportSyntaxError`).
    pub fn xml_error(&mut self, msg: &str) {
        let o = self.xml_offset();
        self.syntax_error(format!("in XML literal: {msg}"), o);
        self.xml_next();
    }

    fn handle_error(&mut self, msg: &str) -> XmlResult<()> {
        if self.ch() == SU {
            return Err(Abort::Truncated);
        }
        self.xml_error(msg);
        Ok(())
    }

    pub fn x_token(&mut self, that: char) -> XmlResult<()> {
        if self.ch() == that {
            self.xml_next();
            Ok(())
        } else {
            let msg = format!("'{that}' expected instead of '{}'", self.ch());
            self.handle_error(&msg)
        }
    }

    pub fn x_tokens(&mut self, that: &str) -> XmlResult<()> {
        that.chars().try_for_each(|c| self.x_token(c))
    }

    pub fn x_space_opt(&mut self) {
        while is_space(self.ch()) {
            self.xml_next();
        }
    }

    pub fn x_space(&mut self) -> XmlResult<()> {
        if is_space(self.ch()) {
            self.xml_next();
            self.x_space_opt();
            Ok(())
        } else {
            self.handle_error("whitespace expected")
        }
    }

    /// `[S] = [S]`.
    pub fn x_eq(&mut self) -> XmlResult<()> {
        self.x_space_opt();
        self.x_token('=')?;
        self.x_space_opt();
        Ok(())
    }

    /// `(Letter | _) {NameChar}`, not ending in `:`.
    pub fn x_name(&mut self) -> XmlResult<String> {
        let ch = self.ch();
        if ch == SU {
            return Err(Abort::Truncated);
        }
        if !is_name_start(ch) {
            self.xml_error(&format!("name expected, but char '{ch}' cannot start a name"));
            return Ok(String::new());
        }
        let mut name = String::new();
        loop {
            name.push(self.ch());
            self.xml_next();
            if !is_name_char(self.ch()) {
                break;
            }
        }
        if name.ends_with(':') {
            self.xml_error("name cannot end in ':'");
            name.pop();
        }
        Ok(name)
    }

    /// An attribute value up to `end`, a quote.
    pub fn x_attribute_value(&mut self, end: char) -> XmlResult<String> {
        let mut value = String::new();
        while self.ch() != end {
            match self.ch() {
                '<' => {
                    self.xml_error("'<' not allowed in attrib value");
                    return Ok(String::new());
                }
                SU => return Err(Abort::Truncated),
                c => {
                    value.push(c);
                    self.xml_next();
                }
            }
        }
        self.xml_next();
        Ok(value)
    }

    /// The characters up to `until`, which is skipped.
    pub fn x_take_until(&mut self, until: &str) -> XmlResult<String> {
        let mut head_rest = until.chars();
        let head = head_rest.next().expect("a terminator");
        let rest = head_rest.as_str();
        let mut text = String::new();
        loop {
            if self.ch() == head && self.s.xml_rest().starts_with(rest) {
                for _ in 0..=rest.chars().count() {
                    self.xml_next();
                }
                return Ok(text);
            }
            if self.ch() == SU {
                return Err(Abort::Truncated);
            }
            text.push(self.ch());
            self.xml_next();
        }
    }
}
