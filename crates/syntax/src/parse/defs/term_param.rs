//! One term parameter, and the `implicit` or `using` modifier of its clause.

use super::super::Parser;
use super::super::migration::Migration;
use super::ParamOwner;
use crate::ast::flags;
use crate::ast::modifiers::{Mod, ModKind, Modifiers};
use crate::ast::{Attrs, NodeId, Span, TName, Tree};
use crate::diag::{Message, rewrite_notice};
use crate::token::Token;

const PARAM_MOD_ADVICE: &str = "It is a keyword only at the beginning of a parameter clause.";

impl Parser<'_> {
    /// `implicit` or `using` at the start of a clause.
    pub fn param_mods(&mut self, initial: &Modifiers, implied: &mut Modifiers) {
        if self.tok() == Token::Implicit {
            let msg = format!(
                "`implicit` parameters are no longer supported, use a `using` clause instead{}",
                rewrite_notice("This construct", "future-migration", "")
            );
            self.migration_error(msg, self.offset(), Migration::Scala2Implicits);
            self.add_param_mod(implied, ModKind::Implicit);
        } else if self.is_ident_named("using") {
            if initial.is(flags::GIVEN) {
                let o = self.offset();
                self.syntax_error("`using` is already implied here, should not be given explicitly", o);
            }
            self.add_param_mod(implied, ModKind::Given);
            if matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
                let msg = Self::expected_but_found_soft(
                    Token::Identifier,
                    Token::ColonOp,
                    "using",
                    PARAM_MOD_ADVICE,
                );
                self.syntax_error_or_incomplete(msg, self.offset());
            }
        }
    }

    fn add_param_mod(&mut self, implied: &mut Modifiers, kind: ModKind) {
        let start = self.s.skip_token();
        let m = Mod { kind, span: Span::new(start, self.last_offset(), start) };
        *implied = self.add_mod(implied.clone(), m);
    }

    /// `{Annotation} [mods] id : ParamType [= Expr]`.
    pub fn param(&mut self, owner: ParamOwner, first: bool, implied: &mut Modifiers) -> NodeId {
        let start = self.offset();
        let annots = self.annotations(false);
        let annotations = self.ast.list(&annots);
        let mut mods = Modifiers { annotations, ..implied.clone() };
        if self.is_consume() || self.is_erased() {
            mods = self.add_modifier(mods);
        }
        if owner.is_class() {
            let m = self.modifiers(crate::token::sets::MODIFIER_TOKENS, mods);
            mods = self.add_flag(m, flags::PARAM_ACCESSOR);
            mods = match self.tok() {
                Token::Val => {
                    self.next();
                    mods
                }
                Token::Var => {
                    let s = self.s.skip_token();
                    let m = Mod { kind: ModKind::Var, span: Span::new(s, self.last_offset(), s) };
                    self.add_mod(mods, m)
                }
                _ => {
                    let rest = flags::PARAM_ACCESSOR.or(flags::INLINE).or(flags::ERASED).or(implied.flags);
                    if !mods.flags.and_not(rest).is_empty() {
                        let o = self.offset();
                        self.syntax_error("`val` or `var` expected", o);
                    }
                    if first && owner == ParamOwner::CaseClass {
                        mods
                    } else {
                        Modifiers { flags: mods.flags.or(flags::PRIVATE_LOCAL), ..mods }
                    }
                }
            };
        } else {
            if self.is_ident_named("inline") && self.s.is_soft_modifier_in_param_modifier_position() {
                mods = self.add_modifier(mods);
            }
            mods.flags = mods.flags.or(flags::PARAM);
        }
        let backquoted = self.tok() == Token::BackquotedIdent;
        let point = if backquoted { self.offset() + 1 } else { self.offset() };
        let name = self.param_name();
        if self.tok() == Token::Arrow && owner.is_class() && !mods.is(flags::LOCAL) {
            let what = if mods.is(flags::MUTABLE) { "var" } else { "val" };
            let msg = Message::syntax(55, format!("{what} parameters may not be call-by-name"));
            let o = self.offset();
            self.syntax_error(msg, o);
        }
        let tpt = self.param_type();
        let rhs = if self.tok() == Token::Equals {
            self.next();
            Some(self.sub_expr())
        } else {
            None
        };
        if !implied.mods.is_empty() {
            implied.mods.clear();
        }
        let mods = self.ast.mods(mods);
        let v = self.ast.node(Tree::ValDef { name, tpt, rhs, mods });
        if backquoted {
            self.ast.add_attrs(v, Attrs::BACKQUOTED);
        }
        self.at_span2(start, point, v)
    }

    /// The name of a parameter and its `:`; `using` without `:` is an error.
    fn param_name(&mut self) -> TName {
        let name = self.ident();
        let colon = matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol);
        if self.s.names.text(name.name) == "using" && !colon {
            let msg = Self::expected_but_found_soft(Token::ColonOp, self.tok(), "using", PARAM_MOD_ADVICE);
            let len = self.s.cur.name.map_or(0, |_| self.s.name_text().encode_utf16().count());
            let end = self.s.source.advance_utf16(self.offset(), len);
            let span = Span::synthetic(self.offset(), end);
            let picked = if self.tok() == Token::Identifier {
                while self.s.is_soft_modifier_in_param_modifier_position() {
                    self.ident();
                }
                self.ident()
            } else {
                name
            };
            self.syntax_error_or_incomplete_span(msg, span);
            return picked;
        }
        self.accept_colon();
        name
    }
}
