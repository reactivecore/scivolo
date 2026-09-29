//! Statement sequences of templates, refinements and blocks.

use super::migration::Migration;
use super::{Location, Parser};
use crate::ast::flags;
use crate::ast::modifiers::{ModKind, Modifiers};
use crate::ast::{NodeId, Tree};
use crate::token::Token;
use crate::token::sets::{
    DCL_INTRO_TOKENS, DEF_INTRO_TOKENS, LOCAL_MODIFIER_TOKENS, MODIFIER_TOKENS, MODIFIER_TOKENS_OR_CASE,
    TokenSet,
};

impl Parser<'_> {
    /// Whether a definition starts here; `no_opaque` excludes the soft modifier `opaque`.
    pub fn is_def_intro(&mut self, allowed: TokenSet, no_opaque: bool) -> bool {
        self.tok() == Token::At
            || DEF_INTRO_TOKENS.contains(self.tok())
            || allowed.contains(self.tok())
            || self.s.is_soft_modifier_in_modifier_position()
                && !(no_opaque && self.s.name_text() == "opaque")
    }

    /// Push `t` unless it is empty (dotty's `+++=`).
    fn push_nonempty(&self, stats: &mut Vec<NodeId>, t: NodeId) {
        if !matches!(self.ast.tree(t), Tree::Empty) {
            stats.push(t);
        }
    }

    /// `[SelfType] TemplateStat {semi TemplateStat}`.
    pub fn template_stat_seq(&mut self) -> (Option<NodeId>, Vec<NodeId>) {
        self.check_no_escaping_placeholders(|p| {
            let mut stats = Vec::new();
            let self_ = p.self_type();
            loop {
                let mut empty = false;
                if p.tok() == Token::Import {
                    stats.extend(p.import_clause(false));
                } else if p.tok() == Token::Export {
                    stats.extend(p.export_clause());
                } else if p.is_ident_named("extension") && p.following_is_extension() {
                    stats.push(p.extension());
                } else if p.is_def_intro(MODIFIER_TOKENS_OR_CASE, false) {
                    let start = p.offset();
                    let mods = p.def_annots_mods(MODIFIER_TOKENS);
                    let t = p.def_or_dcl(start, mods);
                    p.push_nonempty(&mut stats, t);
                } else if p.is_expr_intro() {
                    stats.push(p.expr1(Location::Elsewhere));
                } else {
                    empty = true;
                }
                if !p.stat_sep_or_end(&mut stats, empty, "statement", Token::Eof) {
                    break;
                }
            }
            if stats.is_empty() {
                stats.push(p.empty_tree());
            }
            (self_, stats)
        })
    }

    /// `{ RefineStatSeq }`, or indented if `indent_ok`.
    pub fn refinement(&mut self, indent_ok: bool) -> Vec<NodeId> {
        if indent_ok {
            self.in_braces_or_indented(|p| p.refine_stat_seq())
        } else {
            self.in_braces(|p| p.refine_stat_seq())
        }
    }

    /// `RefineStat {semi RefineStat}`: declarations only.
    pub fn refine_stat_seq(&mut self) -> Vec<NodeId> {
        let mut stats = Vec::new();
        loop {
            let dcl_found = DCL_INTRO_TOKENS.contains(self.tok());
            if dcl_found {
                let start = self.offset();
                let t = self.def_or_dcl(start, Modifiers::default());
                if let Some(msg) = self.refinement_error(t) {
                    let span = self.ast.span(t);
                    self.syntax_error_span(msg, span);
                } else {
                    stats.push(t);
                }
            }
            let mut what = "declaration".to_string();
            if self.in_fun_return_type {
                what += " (possible cause: missing `=` in front of current method body)";
            }
            if !self.stat_sep_or_end(&mut stats, !dcl_found, &what, Token::Eof) {
                return stats;
            }
        }
    }

    fn refinement_error(&self, t: NodeId) -> Option<String> {
        let member_mods = match self.ast.tree(t) {
            Tree::ValDef { mods, .. }
            | Tree::DefDef { mods, .. }
            | Tree::TypeDef { mods, .. }
            | Tree::ModuleDef { mods, .. } => Some(self.ast.modifiers(*mods)),
            _ => None,
        };
        if let Some(m) = member_mods {
            let f = m.flags.and(flags::MODIFIER_FLAGS);
            let mutable_var = m.is(flags::MUTABLE) && m.mods.iter().any(|md| md.kind == ModKind::Var);
            if !f.is_empty() && !mutable_var {
                return Some(format!("refinement cannot be `{}`", f.flag_strings().join("`, `")));
            }
        }
        match *self.ast.tree(t) {
            Tree::DefDef { paramss, .. }
                if self.ast.clause_items(paramss).iter().any(|&c| {
                    self.ast
                        .items(c)
                        .iter()
                        .any(|&p| matches!(self.ast.tree(p), Tree::ValDef { rhs: Some(_), .. }))
                }) =>
            {
                Some("refinement cannot have default arguments".into())
            }
            Tree::ValDef { rhs, .. } | Tree::DefDef { rhs, .. } => {
                rhs.map(|_| "refinement cannot have a right-hand side".into())
            }
            Tree::TypeDef { rhs, .. } => matches!(self.ast.tree(rhs), Tree::Template { .. })
                .then(|| "refinement cannot be a class or trait".into()),
            _ => Some("this kind of definition cannot be a refinement".into()),
        }
    }

    fn local_def(&mut self, start: u32) -> NodeId {
        let mods = self.def_annots_mods(LOCAL_MODIFIER_TOKENS);
        if !mods.is(flags::FINAL) {
            return self.def_or_dcl(start, mods);
        }
        if DCL_INTRO_TOKENS.contains(self.tok()) && self.tok() != Token::Given {
            let o = self.offset();
            self.syntax_error("The final modifier is not allowed on local definitions", o);
        }
        self.tmpl_def(start, mods)
    }

    /// `{BlockStat semi} [Expr]`.
    pub fn block_stat_seq(&mut self) -> Vec<NodeId> {
        self.check_no_escaping_placeholders(|p| {
            let mut stats = Vec::new();
            loop {
                let mut empty = false;
                if p.tok() == Token::Import {
                    stats.extend(p.import_clause(false));
                } else if p.is_expr_intro() {
                    stats.push(p.expr_at(Location::InBlock));
                } else if p.tok() == Token::Implicit && !p.s.in_modifier_position() {
                    let msg = "`implicit` lambdas are no longer supported, use a lambda with `?=>` instead";
                    p.migration_error(msg, p.offset(), Migration::Scala2Implicits);
                    let start = p.offset();
                    let mods = p.modifiers(TokenSet::of(&[Token::Implicit]), Modifiers::default());
                    stats.push(p.closure(start, Location::InBlock, mods));
                } else if p.is_ident_named("extension") && p.following_is_extension() {
                    stats.push(p.extension());
                } else if p.is_def_intro(LOCAL_MODIFIER_TOKENS, true) {
                    let start = p.offset();
                    let t = p.local_def(start);
                    p.push_nonempty(&mut stats, t);
                } else {
                    empty = true;
                }
                if !p.stat_sep_or_end(&mut stats, empty, "statement", Token::Case) {
                    break;
                }
            }
            stats
        })
    }
}
