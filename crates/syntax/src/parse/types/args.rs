// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Type arguments.

use super::super::Parser;
use crate::ast::{NodeId, TName, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// Type arguments: plain, named (`X = T`), or names and types (`x: T`, a named tuple).
    pub fn arg_types(&mut self, named_ok: bool, wild_ok: bool, tuple_ok: bool) -> Vec<NodeId> {
        let check = |p: &mut Self, t: NodeId| if wild_ok { t } else { p.reject_wildcard_type(t) };
        if named_ok && self.is_ident() && self.s.lookahead().token == Token::Equals {
            self.comma_separated(|p| {
                let start = p.offset();
                let name = p.ident();
                p.accept(Token::Equals);
                let t = p.type_arg();
                let arg = check(p, t);
                let n = p.ast.node(Tree::NamedArg { name: TName { is_type: true, ..name }, arg });
                p.at_span1(start, n)
            })
        } else if tuple_ok
            && self.is_ident()
            && matches!(self.s.lookahead().token, Token::ColonOp | Token::ColonFollow | Token::ColonEol)
            && self.s.cfg.source_version.enables_named_tuples()
        {
            self.comma_separated(|p| {
                let start = p.offset();
                let name = p.ident();
                p.accept_colon();
                let t = p.typ();
                let arg = check(p, t);
                let n = p.ast.node(Tree::NamedArg { name, arg });
                p.at_span1(start, n)
            })
        } else {
            self.comma_separated(|p| {
                let t = p.type_arg();
                check(p, t)
            })
        }
    }

    /// A type, or a capture set under capture checking.
    fn type_arg(&mut self) -> NodeId {
        self.capture_set_type(false).unwrap_or_else(|| self.typ())
    }

    /// `[TypeArg {, TypeArg}]`.
    pub fn type_args(&mut self, named_ok: bool, wild_ok: bool) -> Vec<NodeId> {
        self.in_brackets_with_commas(|p| p.arg_types(named_ok, wild_ok, false))
    }
}
