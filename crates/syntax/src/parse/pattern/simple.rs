use super::super::migration::Migration;
use super::super::{Location, Parser};
use super::is_var_pattern_name;
use crate::ast::modifiers::{Mod, ModKind, Modifiers};
use crate::ast::{Attrs, NodeId, Span, Tree};
use crate::token::Token;

impl Parser<'_> {
    pub fn simple_pattern(&mut self) -> NodeId {
        match self.tok() {
            Token::Identifier | Token::BackquotedIdent | Token::This | Token::Super => {
                if self.is_negated_number() {
                    let start = self.s.skip_token();
                    self.literal(start, false, false)
                } else {
                    let r = self.simple_ref();
                    self.simple_pattern_rest(r)
                }
            }
            Token::Uscore => self.wildcard_ident(),
            Token::LParen => {
                let start = self.offset();
                let ps = self.in_parens_with_commas(|p| p.patterns_opt(Location::InPattern));
                let t = self.make_tuple_or_parens(&ps);
                self.at_span1(start, t)
            }
            Token::Quote => self.simple_expr(Location::InPattern),
            Token::XmlStart => self.xml_literal_pattern(),
            Token::Given => {
                let start = self.offset();
                let mod_start = self.s.skip_token();
                let given =
                    Mod { kind: ModKind::Given, span: Span::new(mod_start, self.last_offset(), mod_start) };
                let wildcard = self.term("_");
                let id = self.ast.node(Tree::Ident { name: wildcard });
                let tpt = self.refined_type();
                let typed = self.ast.node(Tree::Typed { expr: id, tpt });
                let mods = self.add_mod(Modifiers::default(), given);
                let mods = self.ast.mods(mods);
                let b = self.ast.node(Tree::Bind { name: wildcard, body: typed, mods });
                self.at_span1(start, b)
            }
            tok if crate::token::sets::LITERAL_TOKENS.contains(tok) => {
                let start = self.offset();
                self.literal(start, true, false)
            }
            _ => {
                let start = self.last_offset();
                let at = self.expected_offset();
                self.syntax_error_or_incomplete("pattern expected", at);
                let name = self.term("_");
                let id = self.ast.node(Tree::Ident { name });
                let end = self.offset();
                self.at_span(Span::synthetic(start, end), id)
            }
        }
    }

    /// Selections, type arguments and argument patterns after `t`.
    fn simple_pattern_rest(&mut self, mut t: NodeId) -> NodeId {
        while self.tok() == Token::Dot {
            self.next();
            t = self.selector(t);
        }
        let mut p = t;
        if self.tok() == Token::LBracket {
            let (start, point) = (self.start_offset(t), self.offset());
            let args = self.type_args(false, false);
            let args = self.ast.list(&args);
            let a = self.ast.node(Tree::TypeApply { fun: p, args });
            p = self.at_span2(start, point, a);
        }
        if self.tok() == Token::LParen {
            let (start, point) = (self.start_offset(t), self.offset());
            let args = self.in_parens_with_commas(|p| p.patterns_opt(Location::InPatternArgs));
            let args = self.ast.list(&args);
            let a = self.ast.node(Tree::Apply { fun: p, args });
            p = self.at_span2(start, point, a);
        }
        if let Tree::Ident { name } | Tree::Select { name, .. } = *self.ast.tree(p) {
            let text = self.s.names.text(name.name).to_string();
            let backquoted = self.ast.get(p).attrs.contains(Attrs::BACKQUOTED);
            let var =
                matches!(self.ast.tree(p), Tree::Ident { .. }) && is_var_pattern_name(&text) && !backquoted;
            if var && text.contains('$') {
                let shown = if text.len() > 1 { text.strip_suffix('$').unwrap_or(&text) } else { &text };
                let msg = format!(
                    "The identifier `{shown}` should not contain `$`, which is reserved for internal compiler use."
                );
                let span = self.ast.span(p);
                self.error_or_migration_warning(msg, span, Migration::IdentifierDollars);
            }
        }
        p
    }
}
