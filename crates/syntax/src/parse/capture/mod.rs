// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Capture checking syntax: capture sets, `^`, and pure arrows `->` and `?->`.

mod annots;

use super::Parser;
use crate::ast::{NodeId, Tree};
use crate::token::Token;
use crate::token::sets::{
    CAN_START_CAPTURE_SET_CONTENTS_TOKENS, CAN_START_INFIX_TYPE_TOKENS, DCL_INTRO_TOKENS,
};

impl Parser<'_> {
    /// Whether the current token is the pure arrow `name` (`->` or `?->`) under pure functions.
    pub fn is_pure_arrow_named(&self, name: &str) -> bool {
        self.s.cfg.features.pure_functions() && self.is_ident_named(name)
    }

    pub fn is_pure_arrow(&self) -> bool {
        self.is_pure_arrow_named("->") || self.is_pure_arrow_named("?->")
    }

    /// Skip `^` after a name under capture checking.
    pub fn gobble_hat(&mut self) -> bool {
        let hat = self.s.cfg.features.capture_checking() && self.is_ident_named("^");
        if hat {
            self.next();
        }
        hat
    }

    /// Whether a `{` here starts a capture set rather than a refinement.
    pub fn at_capture_set(&mut self) -> bool {
        self.s.cfg.features.capture_checking()
            && self.tok() == Token::LBrace
            && !DCL_INTRO_TOKENS.contains(self.s.lookahead().token)
    }

    /// `{SimpleRef .} SimpleRef [*] [. only [QualId]] [. rd]`.
    fn capture_ref(&mut self) -> NodeId {
        let mut r = self.simple_ref();
        loop {
            let r1 = self.reach_opt(r);
            let r1 = self.restricted_opt(r1);
            let r1 = self.read_only_opt(r1);
            if r1 == r && self.tok() == Token::Dot {
                self.next();
                r = self.selector(r);
            } else {
                return r1;
            }
        }
    }

    fn reach_opt(&mut self, r: NodeId) -> NodeId {
        if !self.is_ident_named("*") {
            return r;
        }
        let start = self.start_offset(r);
        self.next();
        let annot = self.internal_annot("reachCapability");
        let t = self.ast.node(Tree::Annotated { arg: r, annot });
        self.at_span1(start, t)
    }

    fn restricted_opt(&mut self, r: NodeId) -> NodeId {
        if !(self.tok() == Token::Dot && self.lookahead_is_ident_named("only")) {
            return r;
        }
        let start = self.start_offset(r);
        self.next();
        self.next();
        let qid = self.in_brackets(|p| {
            let q = p.qual_id();
            p.convert_to_type_id(q)
        });
        let annot = self.only_annot(qid);
        let t = self.ast.node(Tree::Annotated { arg: r, annot });
        self.at_span1(start, t)
    }

    fn read_only_opt(&mut self, r: NodeId) -> NodeId {
        if !(self.tok() == Token::Dot && self.lookahead_is_ident_named("rd")) {
            return r;
        }
        let start = self.start_offset(r);
        self.next();
        self.next();
        let annot = self.internal_annot("readOnlyCapability");
        let t = self.ast.node(Tree::Annotated { arg: r, annot });
        self.at_span1(start, t)
    }

    /// `CaptureRef [initially]` of a `uses` clause.
    pub fn use_ref(&mut self) -> NodeId {
        let reference = self.capture_ref();
        if !self.is_ident_named("initially") {
            return self.ast.node(Tree::UseRef { reference, initially: false });
        }
        let start = self.s.skip_token();
        let t = self.ast.node(Tree::UseRef { reference, initially: true });
        self.at_span1(start, t)
    }

    /// `{ [CaptureRef {, CaptureRef}] }`.
    pub fn capture_set(&mut self) -> Vec<NodeId> {
        self.in_braces(|p| {
            if p.tok() == Token::RBrace { Vec::new() } else { p.comma_separated(Self::capture_ref) }
        })
    }

    /// `[CaptureSet] core` after a pure arrow.
    pub fn captures_and_result(&mut self, core: fn(&mut Self) -> NodeId) -> NodeId {
        let starts_set = self.s.cfg.features.capture_checking()
            && self.tok() == Token::LBrace
            && CAN_START_CAPTURE_SET_CONTENTS_TOKENS.contains(self.s.lookahead().token);
        if !starts_set {
            return core(self);
        }
        let refs = self.capture_set();
        let refs = self.ast.list(&refs);
        let parent = core(self);
        self.ast.node(Tree::CapturesAndResult { refs, parent })
    }

    /// Whether a `^` here is postfix (a capturing type) rather than an infix operator.
    fn is_capture_up_arrow(&mut self) -> bool {
        let la = self.s.lookahead();
        la.token == Token::LBrace
            || self.lookahead_is_ident_named("->")
            || self.lookahead_is_ident_named("?->")
            || !CAN_START_INFIX_TYPE_TOKENS.contains(la.token)
            || la.line_offset.is_some_and(|o| o > 0)
    }

    /// `t ^ [CaptureSet]` if a capturing `^` follows, else `t`.
    pub fn capturing_type(&mut self, t: NodeId) -> NodeId {
        if !(self.s.cfg.features.capture_checking() && self.is_ident_named("^") && self.is_capture_up_arrow())
        {
            return t;
        }
        let start = self.ast.span(t).start();
        self.next();
        let r = if self.tok() == Token::LBrace {
            let refs = self.capture_set();
            self.make_retaining(t, Some(&refs))
        } else {
            self.make_retaining(t, None)
        };
        self.at_span1(start, r)
    }
}
