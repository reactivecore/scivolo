//! Colours of an input as dotty's `SyntaxHighlighting`: a token pass, the comments, then the
//! definitions and types of the parsed statements.

use std::ops::Range;
use std::panic::catch_unwind;
use syntax::ast::tree::ModsId;
use syntax::ast::{Ast, Attrs, NodeId, Span, TName, Tree, flags};
use syntax::config::Config;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::scan::Scanner;
use syntax::source::{Offset, SourceFile};
use syntax::token::Token;
use syntax::token::sets::{ALPHA_KEYWORDS, LITERAL_TOKENS};

const NO_COLOR: &str = "\x1b[0m";
const COMMENT: &str = "\x1b[34m";
const KEYWORD: &str = "\x1b[33m";
const DEFINITION: &str = "\x1b[36m";
const LITERAL: &str = "\x1b[32m";
const TYPE: &str = "\x1b[32m";
const UNDEFINED: &str = "\x1b[41m";

/// `text` with ANSI colours; `text` itself when highlighting fails.
pub fn highlight(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    catch_unwind(|| {
        let mut colors = vec![NO_COLOR; text.len()];
        tokens(text, &mut colors);
        trees(text, &mut colors);
        render(text, &colors)
    })
    .unwrap_or_else(|_| text.to_string())
}

/// Colours `range` of `colors` when it lies inside.
fn fill(colors: &mut [&'static str], range: Range<Offset>, color: &'static str) {
    if range.start <= range.end && range.end as usize <= colors.len() {
        colors[range.start as usize..range.end as usize].fill(color);
    }
}

/// Literals, keywords, `???`, capitalized identifiers, then comments.
fn tokens(text: &str, colors: &mut [&'static str]) {
    let source = SourceFile::new("<highlighting>", text);
    let mut names = Names::default();
    let mut s = Scanner::new(&source, &mut names, Config::default());
    while s.token() != Token::Eof {
        let (start, token) = (s.offset(), s.token());
        let color = match token {
            t if LITERAL_TOKENS.contains(t) || t == Token::StringPart => Some(LITERAL),
            t if ALPHA_KEYWORDS.contains(t) || s.is_soft_modifier_in_modifier_position() => Some(KEYWORD),
            Token::Identifier if s.name_text() == "???" => Some(UNDEFINED),
            Token::Identifier if capitalized(s.name_text()) => Some(KEYWORD),
            _ => None,
        };
        s.next_token();
        let end = s.cur.last_offset - Offset::from(token == Token::StringPart);
        if let Some(color) = color {
            fill(colors, start..end, color);
        }
    }
    for comment in std::mem::take(&mut s.comments) {
        fill(colors, comment, COMMENT);
    }
}

/// Whether the first UTF-16 unit of `name` is an upper-case letter and another one is not.
fn capitalized(name: &str) -> bool {
    let mut upper = name.encode_utf16().map(|u| char::from_u32(u.into()).is_some_and(char::is_uppercase));
    upper.next() == Some(true) && upper.any(|u| !u)
}

/// Definition names, annotations and types of the statements of `text`.
fn trees(text: &str, colors: &mut [&'static str]) {
    let source = SourceFile::new("<highlighting>", text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::default());
    let stats = p.block_stat_seq();
    let walk = Walk { ast: &p.ast, names: &*p.s.names };
    for stat in stats {
        walk.visit(stat, colors);
    }
}

struct Walk<'a> {
    ast: &'a Ast,
    names: &'a Names,
}

impl Walk<'_> {
    fn visit(&self, id: NodeId, colors: &mut [&'static str]) {
        let (ast, span) = (self.ast, self.ast.span(id));
        match ast.tree(id) {
            t if self.name(t).is_some_and(|n| matches!(self.text(n), "<error>" | "<init>")) => {}
            Tree::ValDef { name, mods, .. }
            | Tree::DefDef { name, mods, .. }
            | Tree::TypeDef { name, mods, .. }
            | Tree::ModuleDef { name, mods, .. } => self.definition(id, *name, *mods, colors),
            Tree::Ident { name } if name.is_type => self.fill_span(span, colors, TYPE),
            Tree::Select { qualifier, name } if name.is_type && span.exists() => {
                let (point, len) = (span.point(), self.len(*name));
                let qualifier = ast.span(*qualifier);
                let range = if qualifier.exists() && qualifier.start() > point {
                    span.start()..span.start() + len
                } else if point == span.start() && !span.is_synthetic() && span.end() - point >= 4095 {
                    span.end().saturating_sub(len)..span.end()
                } else {
                    point..span.end()
                };
                fill(colors, range, TYPE);
            }
            Tree::TypeTree | Tree::DerivedTypeTree { .. } => self.fill_span(span, colors, TYPE),
            _ => {}
        }
        let children = match ast.tree(id) {
            Tree::Super { qual, .. } => vec![*qual],
            _ => ast.child_nodes(id),
        };
        for child in children {
            self.visit(child, colors);
        }
    }

    /// The annotations, name and end marker name of definition `id`.
    fn definition(&self, id: NodeId, name: TName, mods: ModsId, colors: &mut [&'static str]) {
        let mods = self.ast.modifiers(mods);
        for &annotation in self.ast.items(mods.annotations) {
            self.fill_span(self.ast.span(annotation), colors, TYPE);
        }
        let span = self.ast.span(id);
        if !span.exists() {
            return;
        }
        let len = self.len(name);
        if !mods.is(flags::SYNTHETIC) && !span.is_synthetic() {
            fill(colors, span.point()..span.point() + len, DEFINITION);
        }
        if self.ast.attrs(id).contains(Attrs::HAS_END_MARKER) {
            fill(colors, span.end().saturating_sub(len)..span.end(), DEFINITION);
        }
    }

    fn fill_span(&self, span: Span, colors: &mut [&'static str], color: &'static str) {
        if span.exists() {
            fill(colors, span.start()..span.end(), color);
        }
    }

    /// The name of a dotty `NameTree`.
    fn name(&self, tree: &Tree) -> Option<TName> {
        match tree {
            Tree::Ident { name }
            | Tree::Select { name, .. }
            | Tree::Bind { name, .. }
            | Tree::ValDef { name, .. }
            | Tree::DefDef { name, .. }
            | Tree::TypeDef { name, .. }
            | Tree::ModuleDef { name, .. } => Some(*name),
            _ => None,
        }
    }

    fn text(&self, name: TName) -> &str {
        self.names.text(name.name)
    }

    /// The length of `name` without one trailing `$` of a longer name; 0 for a fresh `_$n`.
    fn len(&self, name: TName) -> Offset {
        let text = self.text(name);
        match text.strip_prefix("_$") {
            Some(n) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => 0,
            _ if text.len() > 1 && text.ends_with('$') => text.len() as Offset - 1,
            _ => text.len() as Offset,
        }
    }
}

/// `text` with the colour code before each character whose colour differs from the previous
/// one's, and a reset at the end after a coloured character.
fn render(text: &str, colors: &[&'static str]) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut prev = NO_COLOR;
    for (i, c) in text.char_indices() {
        if colors[i] != prev {
            prev = colors[i];
            out.push_str(prev);
        }
        out.push(c);
    }
    if prev != NO_COLOR {
        out.push_str(NO_COLOR);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_changes_only() {
        let colors = [KEYWORD, KEYWORD, DEFINITION, NO_COLOR, LITERAL];
        assert_eq!(render("abcde", &colors), "\x1b[33mab\x1b[36mc\x1b[0md\x1b[32me\x1b[0m");
        assert_eq!(render("ab", &[NO_COLOR, NO_COLOR]), "ab");
        assert_eq!(render("ab", &[LITERAL, NO_COLOR]), "\x1b[32ma\x1b[0mb");
    }

    #[test]
    fn colours_whole_characters() {
        let text = "ü€";
        let colors = [LITERAL, LITERAL, NO_COLOR, NO_COLOR, NO_COLOR];
        assert_eq!(render(text, &colors), "\x1b[32mü\x1b[0m€");
    }

    #[test]
    fn empty_and_plain_inputs_stay() {
        assert_eq!(highlight(""), "");
        assert_eq!(highlight("x + y"), "x + y");
        let mut colors = [NO_COLOR; 2];
        fill(&mut colors, 1..3, KEYWORD);
        assert_eq!(colors, [NO_COLOR; 2]);
    }
}
