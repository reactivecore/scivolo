//! The parser, a port of dotty's `Parsers.Parser`: tokens to untyped trees.

mod annotations;
mod capture;
mod colon_lambda;
mod combinators;
mod defs;
mod depth;
mod errors;
mod expr;
mod idents;
mod imports;
mod infix;
mod interpolation;
mod language;
mod literals;
mod location;
mod lookahead;
mod migration;
mod modifiers;
mod newlines;
mod pattern;
mod precedence;
mod self_type;
mod stat_sep;
mod stats;
mod templates;
mod trees;
mod types;
mod unit;
mod xml;

use crate::ast::{Ast, NodeId, Span, TName};
use crate::config::Config;
use crate::names::{Name, Names};
use crate::scan::Scanner;
use crate::source::{Offset, SourceFile};
use crate::token::Token;
use rustc_hash::FxHashMap;

pub use location::{Location, ParseKind};

/// Quote and splice nesting (dotty's `StageKind`).
pub mod stage {
    pub const QUOTED: u8 = 1;
    pub const SPLICED: u8 = 1 << 1;
    pub const QUOTED_PATTERN: u8 = 1 << 2;
}

pub struct Parser<'a> {
    pub s: Scanner<'a>,
    pub ast: Ast,
    last_error_offset: Option<Offset>,
    /// The parameters introduced by `_` in the current expression, most recent last.
    placeholder_params: Vec<NodeId>,
    op_stack: Vec<infix::OpInfo>,
    in_fun_return_type: bool,
    in_class_constr_annots: bool,
    in_enum: bool,
    in_match_pattern: bool,
    staged: u8,
    fresh: FxHashMap<&'static str, u32>,
    possible_colon_offset: Offset,
    /// Whether a language import set the source version.
    unit_version_set: bool,
    /// `-new-syntax` or `-source future`, as the settings give it.
    new_syntax: bool,
    /// The nesting depth of `nested` calls.
    depth: u32,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a SourceFile, names: &'a mut Names, cfg: Config) -> Self {
        Parser {
            new_syntax: cfg.new_syntax || cfg.source_version.requires_new_syntax(),
            depth: 0,
            s: Scanner::new(source, names, cfg),
            ast: Ast::for_source(source.text.len()),
            last_error_offset: None,
            placeholder_params: Vec::new(),
            op_stack: Vec::new(),
            in_fun_return_type: false,
            in_class_constr_annots: false,
            in_enum: false,
            in_match_pattern: false,
            staged: 0,
            fresh: FxHashMap::default(),
            possible_colon_offset: 0,
            unit_version_set: false,
        }
    }

    fn tok(&self) -> Token {
        self.s.cur.token
    }

    fn offset(&self) -> Offset {
        self.s.cur.offset
    }

    fn last_offset(&self) -> Offset {
        self.s.cur.last_offset
    }

    fn next(&mut self) {
        self.s.next_token();
    }

    pub fn intern(&mut self, text: &str) -> Name {
        self.s.names.intern(text)
    }

    pub fn term(&mut self, text: &str) -> TName {
        TName { name: self.intern(text), is_type: false }
    }

    pub fn type_name(&mut self, text: &str) -> TName {
        TName { name: self.intern(text), is_type: true }
    }

    /// A fresh name `prefix` + counter, unique in this compilation unit.
    fn fresh_name(&mut self, prefix: &'static str) -> Name {
        let n = self.fresh.entry(prefix).or_insert(0);
        *n += 1;
        let text = format!("{prefix}{n}");
        self.intern(&text)
    }

    // Positions ------------------------------------------------------------------------

    fn at_span(&mut self, span: Span, t: NodeId) -> NodeId {
        self.ast.at_span(t, span)
    }

    fn at_span3(&mut self, start: Offset, point: Offset, end: Offset, t: NodeId) -> NodeId {
        self.at_span(Span::new(start, end, point), t)
    }

    /// Span `t` from `start` to the last read offset, if it consumed any input.
    fn at_span2(&mut self, start: Offset, point: Offset, t: NodeId) -> NodeId {
        let last = self.last_offset();
        if last > start { self.at_span3(start, point, last, t) } else { t }
    }

    fn at_span1(&mut self, start: Offset, t: NodeId) -> NodeId {
        self.at_span2(start, start, t)
    }

    fn start_offset(&self, t: NodeId) -> Offset {
        let s = self.ast.span(t);
        if s.exists() { s.start() } else { self.offset() }
    }

    fn point_offset(&self, t: NodeId) -> Offset {
        let s = self.ast.span(t);
        if s.exists() { s.point() } else { self.offset() }
    }

    fn end_offset(&self, t: NodeId) -> Offset {
        let s = self.ast.span(t);
        if s.exists() { s.end() } else { self.last_offset() }
    }

    fn name_start(&self) -> Offset {
        if self.tok() == Token::BackquotedIdent { self.offset() + 1 } else { self.offset() }
    }

    /// The current offset, or the last one if the current token starts a new line.
    fn expected_offset(&self) -> Offset {
        let src = self.s.source;
        if src.offset_to_line(self.offset()) != src.offset_to_line(self.last_offset()) {
            self.last_offset()
        } else {
            self.offset()
        }
    }
}
