// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! Refined, annotated and simple types.

use super::super::Parser;
use super::super::migration::Migration;
use crate::ast::{NodeId, Span, Tree};
use crate::config::Features;
use crate::diag::{Message, rewrite_notice};
use crate::token::Token;

impl Parser<'_> {
    pub fn refined_type(&mut self) -> NodeId {
        let t = self.annot_type();
        let t = self.with_type_rest(t);
        self.refined_type_rest(t)
    }

    /// `{Refinement}` after `t`.
    pub fn refined_type_rest(&mut self, mut t: NodeId) -> NodeId {
        loop {
            self.argument_start();
            if !matches!(self.tok(), Token::LBrace | Token::Indent) {
                return self.capturing_type(t);
            }
            let start = self.start_offset(t);
            let tpt = self.reject_wildcard_type(t);
            let refinements = self.refinement(true);
            let refinements = self.ast.list(&refinements);
            let r = self.ast.node(Tree::RefinedTypeTree { tpt: Some(tpt), refinements });
            t = self.at_span1(start, r);
        }
    }

    /// `{with AnnotType}` after `t`, deprecated in favor of `&`.
    pub fn with_type_rest(&mut self, t: NodeId) -> NodeId {
        if self.tok() != Token::With {
            return t;
        }
        let with_offset = self.offset();
        self.next();
        if matches!(self.tok(), Token::LBrace | Token::Indent) {
            return t;
        }
        let notice = rewrite_notice("This construct", "3.4-migration", "");
        let text = format!("with as a type operator has been deprecated; use & instead{notice}");
        let msg = Message::syntax(3, text);
        self.error_or_migration_warning(
            msg,
            Span::synthetic(with_offset, with_offset + 4),
            Migration::WithOperator,
        );
        let rest = self.annot_type();
        let rest = self.with_type_rest(rest);
        let start = self.start_offset(t);
        let and = self.make_and_type(t, rest);
        self.at_span1(start, and)
    }

    pub fn annot_type(&mut self) -> NodeId {
        let t = self.simple_type();
        self.annot_type_rest(t)
    }

    pub fn annot_type1(&mut self) -> NodeId {
        let t = self.simple_type1();
        self.annot_type_rest(t)
    }

    pub fn annot_type_rest(&mut self, mut t: NodeId) -> NodeId {
        while self.tok() == Token::At {
            let start = self.start_offset(t);
            let arg = self.reject_wildcard_type(t);
            let annot = self.annot();
            let a = self.ast.node(Tree::Annotated { arg, annot });
            t = self.at_span1(start, a);
        }
        t
    }

    /// A literal type, a wildcard `?` (or `_`) with bounds, or `SimpleType1`.
    pub fn simple_type(&mut self) -> NodeId {
        if crate::token::sets::SIMPLE_LITERAL_TOKENS.contains(self.tok()) || self.is_negated_number() {
            let reference = self.simple_literal();
            return self.ast.node(Tree::SingletonTypeTree { reference });
        }
        if let Some(id) = self.kind_projector_underscore() {
            return id;
        }
        if self.tok() == Token::Uscore {
            if !self.in_match_pattern {
                let msg = format!(
                    "`_` is deprecated for wildcard arguments of types: use `?` instead{}",
                    rewrite_notice("This construct", "3.4-migration", "")
                );
                self.migration_error(msg, self.offset(), Migration::WildcardType);
            }
            return self.wildcard_bounds();
        }
        if self.is_ident_named("?") {
            return self.wildcard_bounds();
        }
        let tpt = self.simple_type1();
        if self.s.cfg.features.contains(Features::MODULARITY) && self.tok() == Token::LParen {
            let w = self.wrap_new(tpt);
            self.par_argument_exprss(w)
        } else {
            tpt
        }
    }

    fn wildcard_bounds(&mut self) -> NodeId {
        let start = self.s.skip_token();
        let b = self.type_bounds();
        let span = Span::new(start, self.last_offset(), start);
        self.ast.with_span(b, span);
        b
    }

    /// `id`, `Singleton.id`, `Singleton.type`, `(ArgTypes)`, a refinement, a splice, then
    /// type arguments and projections.
    pub fn simple_type1(&mut self) -> NodeId {
        let t = if self.tok() == Token::LParen {
            let start = self.offset();
            let args = self.in_parens_with_commas(|p| p.arg_types(false, true, true));
            let t = self.make_tuple_or_parens(&args);
            self.at_span1(start, t)
        } else if self.tok() == Token::LBrace {
            let start = self.offset();
            let refinements = self.refinement(false);
            let refinements = self.ast.list(&refinements);
            let t = self.ast.node(Tree::RefinedTypeTree { tpt: None, refinements });
            self.at_span1(start, t)
        } else if self.is_splice() {
            self.splice(true)
        } else {
            let mut t = self.simple_ref();
            loop {
                if self.tok() != Token::Dot {
                    break self.convert_to_type_id(t);
                }
                self.next();
                if self.tok() == Token::Type {
                    self.next();
                    let start = self.start_offset(t);
                    let s = self.ast.node(Tree::SingletonTypeTree { reference: t });
                    break self.at_span1(start, s);
                }
                t = self.selector(t);
            }
        };
        self.simple_type_rest(t)
    }

    /// `#id` and `[TypeArgs]` after `t`.
    pub fn simple_type_rest(&mut self, mut t: NodeId) -> NodeId {
        loop {
            match self.tok() {
                Token::Hash => {
                    self.accept(Token::Hash);
                    let id = self.type_ident();
                    let (start, point) = (self.start_offset(t), self.start_offset(id));
                    let Tree::Ident { name } = *self.ast.tree(id) else { unreachable!() };
                    let s = self.ast.node(Tree::Select { qualifier: t, name });
                    t = self.at_span2(start, point, s);
                }
                Token::LBracket => {
                    let start = self.start_offset(t);
                    let tpt = self.reject_wildcard_type(t);
                    let args = self.type_args(false, true);
                    let a = self.applied_type(tpt, args, start);
                    t = self.at_span1(start, a);
                }
                _ => return self.kind_projector_tuple(t),
            }
        }
    }
}
