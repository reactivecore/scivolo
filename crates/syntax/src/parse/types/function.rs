//! Function types, their parameters, and parenthesized and tuple types.

use super::super::Parser;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, Span, TName, Tree};
use crate::diag::Message;
use crate::source::Offset;
use crate::token::Token;

/// The state `typ` shares with its helpers: the flags of a function type and which of its
/// parameters are erased.
pub struct FunType {
    start: Offset,
    mods: Modifiers,
    erased: Vec<bool>,
}

impl FunType {
    pub fn new(start: Offset) -> FunType {
        FunType { start, mods: Modifiers::default(), erased: Vec::new() }
    }
}

impl Parser<'_> {
    /// `(...)`: function parameters, a tuple, or a parenthesized type.
    pub fn paren_type(&mut self, ft: &mut FunType, in_context_bound: bool) -> NodeId {
        self.next();
        if self.tok() == Token::RParen {
            self.next();
            return self.function_rest(ft, Vec::new());
        }
        let param_start = self.offset();
        self.add_erased(ft);
        let r = self.s.region;
        self.s.regions.get_mut(r).commas_expected = true;
        let first = self.fun_arg_type();
        let colon = matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol);
        let args = match *self.ast.tree(first) {
            Tree::Ident { name } if self.s.names.text(name.name) != "_" && colon => {
                let mods = ft.mods.clone();
                let p = self.typed_fun_param(param_start, TName { is_type: false, ..name }, mods);
                let mut ps = vec![p];
                while self.tok() == Token::Comma {
                    self.next();
                    let start = self.offset();
                    self.add_erased(ft);
                    let (o, name) = (self.offset(), self.ident());
                    let p = self.typed_fun_param(o, name, ft.mods.clone());
                    ps.push(self.at_span1(start, p));
                }
                ps
            }
            _ => {
                let mut ts = vec![first];
                while self.tok() == Token::Comma {
                    self.next();
                    ft.erased.push(false);
                    ts.push(self.fun_arg_type());
                }
                ts
            }
        };
        self.s.regions.get_mut(r).commas_expected = false;
        self.accept(Token::RParen);
        if matches!(self.tok(), Token::Arrow | Token::CtxArrow)
            || self.is_pure_arrow()
            || ft.erased.contains(&true)
        {
            return self.function_rest(ft, args);
        }
        let elems: Vec<NodeId> = args.into_iter().map(|t| self.convert_to_elem(t)).collect();
        let tuple = self.make_tuple_or_parens(&elems);
        let tuple = self.at_span1(ft.start, tuple);
        let t = self.simple_type_rest(tuple);
        let t = self.annot_type_rest(t);
        let t = self.with_type_rest(t);
        let t = self.refined_type_rest(t);
        let t = self.infix_type_rest(t, in_context_bound);
        self.type_rest(ft, t)
    }

    fn add_erased(&mut self, ft: &mut FunType) {
        let erased = self.is_erased();
        ft.erased.push(erased);
        if erased {
            self.next();
        }
    }

    fn convert_to_elem(&mut self, t: NodeId) -> NodeId {
        match *self.ast.tree(t) {
            Tree::ByNameTypeTree { result } => {
                let span = self.ast.span(t);
                let msg = format!("By-name parameter type => {} not allowed here.", self.source_text(result));
                self.syntax_error_span(Message::syntax(22, msg), span);
                result
            }
            Tree::ValDef { name, tpt, .. } => {
                let arg = self.convert_to_elem(tpt);
                let n = self.ast.node(Tree::NamedArg { name, arg });
                let span = self.ast.span(t);
                self.ast.with_span(n, span);
                n
            }
            _ => t,
        }
    }

    pub fn type_rest(&mut self, ft: &mut FunType, t: NodeId) -> NodeId {
        match self.tok() {
            Token::Arrow | Token::CtxArrow => {
                ft.erased.push(false);
                self.function_rest(ft, vec![t])
            }
            Token::Match => self.match_type(t),
            _ if self.is_pure_arrow() => {
                ft.erased.push(false);
                self.function_rest(ft, vec![t])
            }
            Token::ForSome => {
                let o = self.offset();
                let msg =
                    "Existential types are no longer supported -\nuse a wildcard or dependent type instead";
                self.syntax_error(Message::syntax(34, msg), o);
                t
            }
            _ => {
                let with_mods = matches!(self.ast.tree(t), Tree::Function { mods: Some(_), .. });
                if ft.erased.contains(&true) && !with_mods {
                    let msg = "Types with erased keyword can only be function types `(erased ...) => ...`";
                    let msg = Message::syntax(143, msg).without_explanation();
                    self.syntax_error_span(msg, Span::synthetic(ft.start, ft.start + 8));
                }
                t
            }
        }
    }

    /// The `=> Type`, `?=> Type` or `=>> Type` after function parameters `params`.
    fn function_rest(&mut self, ft: &mut FunType, params: Vec<NodeId>) -> NodeId {
        let (start, point) = (ft.start, self.offset());
        let param_span = Span::synthetic(start, self.last_offset());
        let mut token = self.tok();
        let pure = self.is_pure_arrow();
        if pure {
            token = if self.is_ident_named("->") { Token::Arrow } else { Token::CtxArrow };
        } else if token == Token::TLArrow {
            if !ft.mods.flags.is_empty() || params.is_empty() {
                self.syntax_error("illegal parameter list for type lambda", start);
                token = Token::Arrow;
            }
        } else if self.s.cfg.features.pure_functions() {
            ft.mods.flags = ft.mods.flags.or(flags::IMPURE);
        }
        match token {
            Token::CtxArrow => {
                self.next();
                ft.mods.flags = ft.mods.flags.or(flags::GIVEN);
            }
            Token::Arrow | Token::TLArrow => self.next(),
            _ => {
                self.accept(Token::Arrow);
            }
        }
        let result = if pure { self.captures_and_result(Self::typ) } else { self.typ() };
        let t = if token == Token::TLArrow {
            for &p in &params {
                if let Tree::ValDef { tpt, .. } = *self.ast.tree(p)
                    && matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. })
                {
                    let span = self.ast.span(tpt);
                    self.syntax_error_span("parameter of type lambda may not be call-by-name", span);
                }
            }
            let params = self.ast.list(&params);
            self.ast.node(Tree::TermLambdaTypeTree { params, body: result })
        } else if ft.mods.flags.is_one_of(flags::GIVEN.or(flags::IMPURE)) || ft.erased.contains(&true) {
            if ft.mods.is(flags::GIVEN) && params.is_empty() {
                ft.mods.flags = ft.mods.flags.and_not(flags::GIVEN);
                self.syntax_error_span("context function types require at least one parameter", param_span);
            }
            let args = self.ast.list(&params);
            let mods = self.ast.mods(ft.mods.clone());
            let erased = ft.erased.clone().into_boxed_slice();
            self.ast.node(Tree::Function { args, body: result, mods: Some((mods, erased)), wildcard: false })
        } else {
            self.function_type(&params, result)
        };
        self.at_span2(start, point, t)
    }
}
