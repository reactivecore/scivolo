// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/xml/MarkupParsers.scala
//   compiler/src/dotty/tools/dotc/parsing/xml/SymbolicXMLBuilder.scala
//! XML literals in expressions and patterns: a port of dotty's `MarkupParsers` and
//! `SymbolicXMLBuilder`, which build `scala.xml` trees.

mod attributes;
mod builder;
mod char_ref;
mod content;
mod element;
mod lexical;
mod pattern;

use super::Parser;
use super::migration::Migration;
use crate::ast::{NodeId, Span};
use crate::source::Offset;
use crate::token::Token;

/// The XML parser's state across the literals of a unit.
pub struct XmlState {
    /// Whether the text just read ended at a `{` opening a Scala block.
    embedded_block: bool,
    /// The position of the content being read.
    tmppos: Span,
    /// The open start tags: offset and name.
    start_tags: Vec<(Offset, String)>,
    /// Whether trees are built for a pattern.
    is_pattern: bool,
    /// Whether the current character is the low surrogate of a supplementary character.
    low_half: bool,
}

impl Default for XmlState {
    fn default() -> Self {
        XmlState {
            embedded_block: false,
            tmppos: Span::NONE,
            start_tags: Vec::new(),
            is_pattern: false,
            low_half: false,
        }
    }
}

/// Why reading a literal stopped (dotty's control throwables).
#[derive(Debug)]
pub enum Abort {
    Truncated,
    MissingEndTag,
    ConfusedAboutBraces,
    /// Elements nested too deeply, at this offset.
    TooDeep(Offset),
}

pub type XmlResult<T> = Result<T, Abort>;

impl Parser<'_> {
    /// An XML literal at the current `XmlStart` token.
    pub fn xml_literal(&mut self) -> NodeId {
        let t = self.xml_literal_common(false);
        self.xml_deprecation(t)
    }

    /// An XML pattern at the current `XmlStart` token.
    pub fn xml_literal_pattern(&mut self) -> NodeId {
        let t = self.xml_literal_common(true);
        self.xml_deprecation(t)
    }

    fn xml_deprecation(&mut self, t: NodeId) -> NodeId {
        let msg = "XML literals are no longer supported.\nSee https://docs.scala-lang.org/scala3/reference/dropped-features/xml.html";
        let span = self.ast.span(t);
        self.error_or_migration_warning(msg, span, Migration::XmlLiteral);
        t
    }

    /// Read the literal from the characters after `<`, then resume scanning after it.
    fn xml_literal_common(&mut self, pattern: bool) -> NodeId {
        let saved = self.s.cur;
        let result = if pattern { self.x_literal_pattern() } else { self.x_literal() };
        self.xml.low_half = false;
        let mut too_deep = None;
        let output = match result {
            Ok(t) => Some(t),
            Err(Abort::TooDeep(offset)) => {
                too_deep = Some(offset);
                None
            }
            Err(Abort::Truncated) => {
                let msg = "input ended while parsing XML";
                if pattern {
                    let o = self.xml_offset();
                    self.syntax_error(msg, o);
                } else {
                    self.incomplete_input_error(msg);
                }
                None
            }
            Err(abort) => {
                let (pos, tag) = self.xml.start_tags.last().cloned().unwrap_or_default();
                let msg = match abort {
                    Abort::MissingEndTag => format!("start tag was here: {tag}>"),
                    _ => format!(
                        " I encountered a '}}' where I didn't expect one, maybe this tag isn't closed <{tag}>"
                    ),
                };
                self.syntax_error(msg, pos);
                None
            }
        };
        self.s.resume(saved);
        if let Some(offset) = too_deep {
            self.give_up(offset);
        }
        output.unwrap_or_else(|| {
            let o = self.offset();
            self.error_term_tree(o)
        })
    }

    /// `element {element}`.
    fn x_literal(&mut self) -> XmlResult<NodeId> {
        self.xml.is_pattern = false;
        let mut ts = Vec::new();
        let start = self.xml_offset() - 1;
        self.xml.tmppos = Span::at(self.xml_offset());
        self.content_lt(&mut ts)?;
        if self.char_coming_after(Self::x_space_opt) != '<' {
            return Ok(ts[0]);
        }
        loop {
            self.x_space_opt();
            self.xml_next();
            self.content_lt(&mut ts)?;
            if self.char_coming_after(Self::x_space_opt) != '<' {
                break;
            }
        }
        let span = Span::new(start, self.xml_offset(), start);
        Ok(self.make_xml_seq(span, &ts, false))
    }

    fn x_literal_pattern(&mut self) -> XmlResult<NodeId> {
        let saved = std::mem::replace(&mut self.xml.is_pattern, true);
        let result = self.x_pattern().inspect(|_| self.x_space_opt());
        self.xml.is_pattern = saved;
        result
    }

    /// Run `f` one nesting level deeper, or abort beyond the limit.
    fn xml_nested<T>(&mut self, f: impl FnOnce(&mut Self) -> XmlResult<T>) -> XmlResult<T> {
        if !self.enter_nesting() {
            return Err(Abort::TooDeep(self.xml_offset()));
        }
        let result = f(self);
        self.leave_nesting();
        result
    }

    /// The character after running `body` on a copy of the reader.
    fn char_coming_after(&mut self, body: fn(&mut Self)) -> char {
        let (mark, low_half) = (self.s.xml_mark(), self.xml.low_half);
        body(self);
        let c = self.ch();
        self.s.xml_reset(mark);
        self.xml.low_half = low_half;
        c
    }

    /// Parse `op` as Scala code after a `{` in the XML; `kind` names it in an error.
    fn escape_to_scala<T>(&mut self, op: impl FnOnce(&mut Self) -> T, kind: &str) -> T {
        self.xml.embedded_block = false;
        let region = self.s.region;
        self.s.xml_open_block();
        let res = op(self);
        self.s.region = region;
        if self.tok() != Token::RBrace {
            self.xml_error(&format!(" expected end of Scala {kind}"));
        }
        res
    }

    fn x_embedded_expr(&mut self) -> NodeId {
        self.escape_to_scala(|p| p.block(false), "block")
    }

    fn x_scala_patterns(&mut self) -> Vec<NodeId> {
        self.escape_to_scala(|p| p.patterns(super::Location::InPattern), "pattern")
    }
}
