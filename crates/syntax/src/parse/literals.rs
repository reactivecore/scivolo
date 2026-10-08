use super::{Parser, stage};
use crate::ast::constant::Constant;
use crate::ast::from_digits;
use crate::ast::tree::NumberKind;
use crate::ast::{NodeId, Tree};
use crate::diag::Message;
use crate::source::Offset;
use crate::token::Token;
use crate::token::sets::NUMERIC_LIT_TOKENS;

impl Parser<'_> {
    pub fn is_negated_number(&mut self) -> bool {
        self.is_ident_named("-") && NUMERIC_LIT_TOKENS.contains(self.s.lookahead().token)
    }

    /// `[-] literal` in a type or singleton position.
    pub fn simple_literal(&mut self) -> NodeId {
        if self.is_ident_named("-") {
            let start = self.s.skip_token();
            self.literal(start, false, true)
        } else {
            let start = self.offset();
            self.literal(start, false, true)
        }
    }

    /// The string value of the current token, empty if it has none.
    pub fn str_val(&self) -> &[u16] {
        self.s.str_val(&self.s.cur).unwrap_or(&[])
    }

    /// The digits of the current numeric literal without `_`, after a `-` if `negated`.
    fn digits(&self, negated: bool) -> String {
        let raw = String::from_utf16_lossy(self.str_val()).replace('_', "");
        if negated { format!("-{raw}") } else { raw }
    }

    /// The current literal token as a tree; `start` is before a `-` sign, if any.
    pub fn literal(&mut self, start: Offset, in_pattern: bool, in_type_or_singleton: bool) -> NodeId {
        let t = if self.tok() == Token::QuoteId {
            self.quoted_id_or_symbol()
        } else if self.tok() == Token::InterpolationId {
            return self.interpolated_string(in_pattern, start);
        } else {
            let t = self.literal_of(start, in_type_or_singleton);
            self.next();
            t
        };
        self.at_span1(start, t)
    }

    fn literal_of(&mut self, start: Offset, in_type_or_singleton: bool) -> NodeId {
        let token = self.tok();
        let negated = start < self.offset();
        if !in_type_or_singleton {
            if negated && start < self.offset() - 1 {
                self.warning(Message::syntax(28, "Illegal literal"), start);
            }
            let kind = match token {
                Token::IntLit => Some(NumberKind::Whole(self.s.cur.base)),
                Token::DeciLit => Some(NumberKind::Decimal),
                Token::ExpoLit => Some(NumberKind::Floating),
                _ => None,
            };
            if let Some(kind) = kind {
                let digits = self.digits(negated).into();
                return self.ast.node(Tree::Number { digits, kind });
            }
        }
        let base = self.s.cur.base as u32;
        let value = match token {
            Token::IntLit => from_digits::int(&self.digits(negated), base).map(Constant::Int),
            Token::LongLit => from_digits::long(&self.digits(negated), base).map(Constant::Long),
            Token::FloatLit => from_digits::float(&self.digits(negated)).map(Constant::Float),
            Token::DoubleLit | Token::DeciLit | Token::ExpoLit => {
                from_digits::double(&self.digits(negated)).map(Constant::Double)
            }
            Token::CharLit => Ok(Constant::Char(self.str_val().first().copied().unwrap_or(0))),
            Token::StringLit | Token::StringPart => Ok(Constant::String(self.str_val().into())),
            Token::True => Ok(Constant::Boolean(true)),
            Token::False => Ok(Constant::Boolean(false)),
            Token::Null => Ok(Constant::Null),
            _ => {
                self.syntax_error_or_incomplete(Message::syntax(28, "Illegal literal"), self.offset());
                Ok(Constant::Null)
            }
        };
        let value = value.unwrap_or_else(|msg| {
            self.syntax_error_or_incomplete(msg, self.offset());
            Constant::Null
        });
        self.ast.node(Tree::Literal { value })
    }

    /// `'x`: a quoted identifier inside a splice, else a symbol literal.
    fn quoted_id_or_symbol(&mut self) -> NodeId {
        let name = self.s.name_text().to_string();
        if self.staged & stage::SPLICED != 0
            && name.chars().next().is_some_and(crate::scan::is_identifier_start)
        {
            let start = self.offset() + 1;
            let t = match self.s.cur.name.and_then(crate::names::Name::keyword) {
                Some(tok @ (Token::True | Token::False | Token::Null)) => {
                    let value = match tok {
                        Token::True => Constant::Boolean(true),
                        Token::False => Constant::Boolean(false),
                        _ => Constant::Null,
                    };
                    self.ast.node(Tree::Literal { value })
                }
                Some(Token::This) => self.ast.node(Tree::This { qual: None }),
                _ => {
                    let name = self.term(&name);
                    self.ast.node(Tree::Ident { name })
                }
            };
            let t = self.at_span1(start, t);
            self.next();
            self.ast.node(Tree::Quote { body: t })
        } else {
            if !self.s.cfg.features.contains(crate::config::Features::SYMBOL_LITERALS) {
                let msg = format!(
                    "symbol literal '{name} is no longer supported,\nuse a string literal \"{name}\" or an application Symbol(\"{name}\") instead,\nor enclose in braces '{{{name}}} if you want a quoted expression.\nFor now, you can also `import language.deprecated.symbolLiterals` to accept\nthe idiom, but this possibility might no longer be available in the future."
                );
                self.migration_error(msg, self.offset(), super::migration::Migration::Scala2to3);
            }
            let start = self.s.skip_token();
            let value = self.str_val().into();
            let t = self.ast.node(Tree::SymbolLit { value });
            self.at_span1(start, t)
        }
    }
}
