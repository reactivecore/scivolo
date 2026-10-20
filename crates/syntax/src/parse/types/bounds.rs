// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Bounds, context bounds, and the types of parameters.

use super::super::Parser;
use super::super::migration::Migration;
use crate::ast::{NodeId, Span, TName, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// `[>: Type] [<: Type]`.
    pub fn type_bounds(&mut self) -> NodeId {
        let start = self.offset();
        let lo = self.bound(Token::Supertype);
        let hi = self.bound(Token::Subtype);
        let t = self.ast.node(Tree::TypeBoundsTree { lo, hi, alias: None });
        self.at_span1(start, t)
    }

    fn bound(&mut self, tok: Token) -> Option<NodeId> {
        if self.tok() != tok {
            return None;
        }
        self.next();
        Some(self.capture_set_type(tok == Token::Supertype).unwrap_or_else(|| self.toplevel_typ()))
    }

    /// `TypeBounds [: ContextBounds]` of type parameter `pname`.
    pub fn type_and_ctx_bounds(&mut self, pname: TName) -> NodeId {
        let t = self.type_bounds();
        let cbs = self.context_bounds(pname);
        let Some(&first) = cbs.first() else { return t };
        let start = self.ast.span(t).union(self.ast.span(first)).start();
        let cx_bounds = self.ast.list(&cbs);
        let c = self.ast.node(Tree::ContextBounds { bounds: t, cx_bounds });
        self.at_span1(start, c)
    }

    /// `Type [as id]`.
    fn context_bound(&mut self, pname: TName) -> NodeId {
        let start = self.offset();
        let t = self.toplevel_typ_in_context_bound();
        let own = if self.is_ident_named("as") && self.s.cfg.source_version.enables_new_givens() {
            self.next();
            self.ident()
        } else {
            self.term("")
        };
        let c = self.ast.node(Tree::ContextBoundTypeTree { tycon: t, param_name: pname, own_name: own });
        let ts = self.ast.span(t);
        let span = if ts.exists() {
            Span::new(ts.start(), self.last_offset(), ts.end())
        } else {
            Span::new(start, self.last_offset(), start)
        };
        self.ast.with_span(c, span);
        c
    }

    /// `: ContextBound {: ContextBound}`, `: {ContextBound, ...}`, or legacy view bounds.
    pub fn context_bounds(&mut self, pname: TName) -> Vec<NodeId> {
        if matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
            self.next();
            if self.tok() == Token::LBrace && self.s.cfg.source_version.enables_new_givens() {
                return self.in_braces(|p| p.comma_separated(|p| p.context_bound(pname)));
            }
            let bound = self.context_bound(pname);
            let mut out = vec![bound];
            if matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
                self.migration_error(
                    "Multiple context bounds should be enclosed in `{ ... }`",
                    self.offset(),
                    Migration::GivenSyntax,
                );
                out.extend(self.context_bounds(pname));
            }
            out
        } else if self.tok() == Token::ViewBound {
            let msg = "view bounds `<%' are no longer supported, use a context bound `:' instead";
            self.migration_error(msg, self.offset(), Migration::Scala2to3);
            let start = self.s.skip_token();
            let id = self.ast.node(Tree::Ident { name: pname });
            let args = self.ast.list(&[id]);
            let body = self.toplevel_typ();
            let f = self.ast.node(Tree::Function { args, body, mods: None, wildcard: false });
            let mut out = vec![self.at_span1(start, f)];
            out.extend(self.context_bounds(pname));
            out
        } else {
            Vec::new()
        }
    }

    /// `core`, or `=> core` (`-> [CaptureSet] core`) as a by-name type.
    pub fn param_type_of(&mut self, core: fn(&mut Self) -> NodeId) -> NodeId {
        let impure = self.tok() == Token::Arrow;
        if !impure && !self.is_pure_arrow_named("->") {
            return core(self);
        }
        let start = self.s.skip_token();
        let result = if !impure {
            self.captures_and_result(core)
        } else if self.s.cfg.features.pure_functions() {
            let tp = core(self);
            self.impure_result(tp)
        } else {
            core(self)
        };
        let t = self.ast.node(Tree::ByNameTypeTree { result });
        self.at_span1(start, t)
    }

    /// `[=>] Type`.
    pub fn fun_arg_type(&mut self) -> NodeId {
        self.param_type_of(Self::typ)
    }

    /// `[=>] Type [*]`.
    pub fn param_type(&mut self) -> NodeId {
        self.param_type_of(Self::param_value_type)
    }

    /// `Type [*]`.
    pub fn param_value_type(&mut self) -> NodeId {
        let t = self.toplevel_typ();
        if !self.is_ident_named("*") {
            return t;
        }
        self.next();
        let start = self.start_offset(t);
        let star = self.type_name("*");
        let op = self.ast.node(Tree::Ident { name: star });
        let pf = self.ast.node(Tree::PostfixOp { od: t, op });
        self.at_span1(start, pf)
    }

    /// A type parameter without context bounds, which are reported (a new, unpositioned
    /// definition without modifiers, as in dotty).
    pub fn strip_context_bounds(&mut self, tparam: NodeId) -> NodeId {
        if let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(tparam)
            && let Tree::ContextBounds { bounds, .. } = *self.ast.tree(rhs)
        {
            let span = self.ast.span(rhs);
            self.syntax_error_span("context bounds are not allowed in type lambdas", span);
            let mods = self.empty_mods();
            return self.ast.node(Tree::TypeDef { name, rhs: bounds, mods });
        }
        tparam
    }
}
