// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
use super::Parser;
use crate::ast::flags;
use crate::ast::{NodeId, TName, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// `id [: InfixType] =>` or `this : InfixType =>`, or none.
    pub fn self_type(&mut self) -> Option<NodeId> {
        if !(self.is_ident() || self.tok() == Token::This) {
            return None;
        }
        let la = self.s.lookahead();
        if !(la.is_colon() && self.following_is_self_type() || la.token == Token::Arrow) {
            return None;
        }
        let start = self.offset();
        let name = if self.tok() == Token::This {
            self.next();
            self.term("_")
        } else {
            self.ident()
        };
        let wildcard = self.s.names.text(name.name) == "_";
        let tpt = if self.s.cur.is_colon() {
            self.next();
            self.infix_type()
        } else {
            if wildcard {
                self.accept(Token::ColonFollow);
            }
            self.type_tree()
        };
        if self.tok() == Token::Arrow {
            self.s.cur.token = Token::SelfArrow;
            self.next();
        } else {
            let o = self.offset();
            self.syntax_error("`=>` expected after self type", o);
        }
        let mods = self.mods_with(flags::PRIVATE_LOCAL);
        let v = self.ast.node(Tree::ValDef { name: TName { is_type: false, ..name }, tpt, rhs: None, mods });
        Some(self.at_span1(start, v))
    }
}
