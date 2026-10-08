use super::super::{Location, Parser};
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, Span, TName, Tree};
use crate::diag::Message;
use crate::token::Token;

impl Parser<'_> {
    /// dotty's `IllegalStartSimpleExpr` message.
    fn illegal_start_simple_expr(tok: Token) -> Message {
        Message::syntax(18, format!("expression expected but {} found", tok.show()))
    }

    pub fn simple_expr(&mut self, location: Location) -> NodeId {
        let mut can_apply = true;
        let t = match self.tok() {
            Token::XmlStart => self.xml_literal(),
            Token::Identifier if self.is_splice() => self.splice(false),
            Token::Identifier | Token::BackquotedIdent | Token::This | Token::Super => self.simple_ref(),
            Token::Uscore => {
                let start = self.s.skip_token();
                let name = TName { name: self.fresh_name("_$"), is_type: false };
                let tpt = self.type_tree();
                let mods = self.mods_with(flags::SYNTHETIC_TERM_PARAM);
                let param = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
                self.ast.with_span(param, Span::at(start));
                self.placeholder_params.push(param);
                let id = self.ast.node(Tree::Ident { name });
                self.at_span1(start, id)
            }
            Token::LParen => {
                let start = self.offset();
                let ts = self.in_parens_with_commas(|p| p.exprs_in_parens_or_bindings());
                let t = self.make_tuple_or_parens(&ts);
                self.at_span1(start, t)
            }
            Token::LBrace | Token::Indent => {
                can_apply = false;
                self.block_expr()
            }
            Token::Quote => self.quote(location.in_pattern()),
            Token::New => {
                can_apply = false;
                self.new_expr()
            }
            Token::Macro => {
                self.next();
                let expr = self.simple_expr(Location::Elsewhere);
                self.ast.node(Tree::MacroTree { expr })
            }
            tok => {
                if crate::token::sets::LITERAL_TOKENS.contains(tok) {
                    let start = self.offset();
                    self.literal(start, false, false)
                } else if matches!(tok, Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
                    let o = self.offset();
                    self.syntax_error(Self::illegal_start_simple_expr(tok), o);
                    self.next();
                    self.simple_expr(location)
                } else {
                    let start = self.last_offset();
                    let msg = Self::illegal_start_simple_expr(tok);
                    let at = self.expected_offset();
                    self.syntax_error_or_incomplete(msg, at);
                    self.error_term_tree(start)
                }
            }
        };
        self.simple_expr_rest(t, location, can_apply)
    }

    /// Selections, type arguments, arguments and `_` after a simple expression `t`.
    pub fn simple_expr_rest(&mut self, mut t: NodeId, location: Location, mut can_apply: bool) -> NodeId {
        loop {
            if can_apply {
                self.argument_start();
            }
            match self.tok() {
                Token::Dot => {
                    if let Tree::Number { digits, .. } = self.ast.tree(t)
                        && digits.starts_with('-')
                    {
                        let start = self.ast.span(t).start();
                        self.warning(Message::syntax(28, "Illegal literal"), start);
                    }
                    self.next();
                    t = self.selector_or_match(t);
                    can_apply = true;
                }
                Token::LBracket => {
                    let (start, point) = (self.start_offset(t), self.offset());
                    let targs = self.type_args(true, false);
                    let args = self.ast.list(&targs);
                    let app = self.ast.node(Tree::TypeApply { fun: t, args });
                    t = self.at_span2(start, point, app);
                    can_apply = true;
                }
                Token::LParen | Token::LBrace | Token::Indent if can_apply => {
                    let (start, point) = (self.start_offset(t), self.offset());
                    let args = self.argument_exprs();
                    let app = self.mk_apply(t, args);
                    t = self.at_span2(start, point, app);
                    can_apply = true;
                }
                Token::Uscore => {
                    let start = self.start_offset(t);
                    let point = self.s.skip_token();
                    let w = self.term("_");
                    let op = self.ast.node(Tree::Ident { name: w });
                    let pf = self.ast.node(Tree::PostfixOp { od: t, op });
                    return self.at_span2(start, point, pf);
                }
                _ => {
                    let colon = matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol);
                    if colon && location == Location::InParens && self.following_is_lambda_params() {
                        return self.lambda_param_after_colon(t);
                    }
                    let Some(lambda) = self.detect_colon_lambda() else { return t };
                    let start = self.start_offset(t);
                    let point = self.s.skip_token();
                    let arg = self.colon_lambda(lambda);
                    let args = self.ast.list(&[arg]);
                    let app = self.ast.node(Tree::Apply { fun: t, args });
                    t = self.at_span2(start, point, app);
                    can_apply = true;
                }
            }
        }
    }

    /// `(x: T) => ...`: the identifier before `:` is a lambda parameter.
    fn lambda_param_after_colon(&mut self, t: NodeId) -> NodeId {
        let Tree::Ident { name } = *self.ast.tree(t) else { return t };
        let text = self.s.names.text(name.name);
        if text.starts_with("_$")
            && self
                .placeholder_params
                .last()
                .is_some_and(|&p| matches!(self.ast.tree(p), Tree::ValDef { name: n, .. } if *n == name))
        {
            self.placeholder_params.pop();
        }
        let start = self.start_offset(t);
        let backquoted = self.ast.get(t).attrs.contains(Attrs::BACKQUOTED);
        let tpt = self.typed_opt();
        let p = self.make_parameter(TName { is_type: false, ..name }, tpt, Modifiers::default(), backquoted);
        self.at_span1(start, p)
    }

    /// `$x` or `${...}` outside quotes, or a `$`-prefixed identifier inside one.
    pub fn is_splice(&mut self) -> bool {
        self.tok() == Token::Identifier && {
            let name = self.s.name_text();
            name.starts_with('$')
                && if name.len() == 1 {
                    self.s.lookahead().token == Token::LBrace
                } else {
                    self.staged & super::super::stage::QUOTED != 0
                }
        }
    }
}
