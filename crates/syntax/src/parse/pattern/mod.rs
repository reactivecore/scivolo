//! Patterns.

mod simple;

use super::migration::Migration;
use super::{Location, ParseKind, Parser};
use crate::ast::flags;
use crate::ast::{NodeId, Tree};
use crate::token::Token;
use crate::token::sets::CAN_START_PATTERN_TOKENS;
use unicode_general_category::{GeneralCategory as G, get_general_category};

/// Whether `name` is the name of a variable pattern: it starts with a lower case letter or `_`.
pub fn is_var_pattern_name(name: &str) -> bool {
    let Some(first) = name.chars().next() else { return false };
    (first == '_' || get_general_category(first) == G::LowercaseLetter)
        && !matches!(name, "false" | "true" | "null")
}

impl Parser<'_> {
    /// `Pattern1 {| Pattern1}`.
    pub fn pattern(&mut self, location: Location) -> NodeId {
        self.nested(|p| p.pattern_at_level(location))
    }

    fn pattern_at_level(&mut self, location: Location) -> NodeId {
        let pat = self.pattern1(location);
        if !self.is_ident_named("|") {
            return pat;
        }
        let mut alts = vec![pat];
        while self.is_ident_named("|") {
            self.next();
            alts.push(self.pattern1(location));
        }
        let start = self.start_offset(pat);
        let trees = self.ast.list(&alts);
        let a = self.ast.node(Tree::Alternative { trees });
        self.at_span1(start, a)
    }

    /// `Pattern2 [: Type]`.
    pub fn pattern1(&mut self, location: Location) -> NodeId {
        let pat = self.pattern2(location);
        let at_colon = matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol);
        if !at_colon {
            return pat;
        }
        self.next();
        let t = self.ascription(pat, location);
        self.check_ascription(pat, t, true);
        t
    }

    /// Warn about ascriptions after patterns other than variables and numbers.
    pub fn check_ascription(&mut self, pat: NodeId, t: NodeId, in_pattern: bool) {
        match *self.ast.tree(pat) {
            Tree::Ident { name } => {
                let text = self.s.names.text(name.name).to_string();
                if in_pattern && !is_var_pattern_name(&text) {
                    let tpt = match *self.ast.tree(t) {
                        Tree::Typed { tpt, .. } => self.source_text(tpt),
                        _ => "T".to_string(),
                    };
                    let span = self.ast.span(pat);
                    let msg = format!(
                        "Typed pattern is not a variable pattern but could be written `{text} @ (_: {tpt})`"
                    );
                    self.s.diags.warning_at(span.start(), span.end(), span.point(), msg);
                }
            }
            Tree::Number { .. } => {}
            _ => {
                let msg = "Type ascriptions after patterns other than:\n  * variable pattern, e.g. `case x: String =>`\n  * number literal pattern, e.g. `case 10.5: Double =>`\nare no longer supported. Remove the type ascription or move it to a separate variable pattern.";
                let span = self.ast.span(pat);
                self.error_or_migration_warning(msg, span, Migration::AscriptionAfterPattern);
            }
        }
    }

    /// The source text of `t`'s span.
    pub fn source_text(&self, t: NodeId) -> String {
        let s = self.ast.span(t);
        if !s.exists() {
            return String::new();
        }
        self.s.source.text.get(s.start() as usize..s.end() as usize).unwrap_or("").to_string()
    }

    /// `InfixPattern [*]`.
    fn pattern3(&mut self, location: Location) -> NodeId {
        let p = self.infix_pattern();
        if !self.following_is_vararg() {
            return p;
        }
        let start = self.s.skip_token();
        if !location.in_args() {
            self.syntax_error("bad use of `*` - sequence pattern not allowed here", start);
            return p;
        }
        match *self.ast.tree(p) {
            Tree::Ident { name } if is_var_pattern_name(self.s.names.text(name.name)) => {
                let star = self.type_name("_*");
                let id = self.ast.node(Tree::Ident { name: star });
                let id = self.at_span1(start, id);
                self.ast.node(Tree::Typed { expr: p, tpt: id })
            }
            _ => {
                self.syntax_error("`*` must follow pattern variable", start);
                p
            }
        }
    }

    /// `[id @] Pattern3`.
    pub fn pattern2(&mut self, location: Location) -> NodeId {
        let p = self.pattern3(location);
        let Tree::Ident { name } = *self.ast.tree(p) else { return p };
        if self.tok() != Token::At {
            return p;
        }
        self.s.skip_token();
        let pt = self.pattern3(location);
        let start = self.start_offset(p);
        let t = match self.ast.tree(pt).clone() {
            Tree::Bind { name: b, body, mods }
                if self.s.names.text(b.name) == "_"
                    && matches!(self.ast.tree(body), Tree::Typed { .. })
                    && self.ast.modifiers(mods).is(flags::GIVEN) =>
            {
                self.ast.node(Tree::Bind { name, body, mods })
            }
            Tree::Typed { expr, tpt }
                if matches!(self.ast.tree(expr), Tree::Ident { name } if self.s.names.text(name.name) == "_")
                    && matches!(self.ast.tree(tpt), Tree::Ident { name } if self.s.names.text(name.name) == "_*") =>
            {
                self.ast.node(Tree::Typed { expr: p, tpt })
            }
            _ => {
                let mods = self.empty_mods();
                self.ast.node(Tree::Bind { name, body: pt, mods })
            }
        };
        self.at_span2(start, 0, t)
    }

    /// `SimplePattern {id [nl] SimplePattern}`.
    fn infix_pattern(&mut self) -> NodeId {
        let first = self.simple_pattern();
        self.infix_ops(
            first,
            |p, tok| p.s.can_start_expr_tokens().contains(tok),
            |p, _| p.simple_pattern(),
            Location::InPattern,
            ParseKind::Pattern,
            |p| {
                (p.s.cur.name.is_none() || p.s.name_text() != "|")
                    && !p.following_is_vararg()
                    && p.next_can_follow_operator(CAN_START_PATTERN_TOKENS)
            },
        )
    }

    /// `Pattern {, Pattern}`, each possibly named.
    pub fn patterns(&mut self, location: Location) -> Vec<NodeId> {
        self.comma_separated(|p| p.maybe_named(|p| p.pattern(location)))
    }

    pub fn patterns_opt(&mut self, location: Location) -> Vec<NodeId> {
        if self.tok() == Token::RParen { Vec::new() } else { self.patterns(location) }
    }
}
