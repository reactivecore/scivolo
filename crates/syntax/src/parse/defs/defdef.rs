// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
use super::ParamOwner;
use crate::ast::constant::Constant;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, List, NodeId, Tree};
use crate::diag::{Message, rewrite_notice};
use crate::parse::Parser;
use crate::parse::migration::Migration;
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// `DefSig [: Type] [= Expr]` or `this ParamClauses = ConstrExpr` after `def`.
    pub fn def_def_or_dcl(&mut self, start: Offset, mods: Modifiers, num_lead: usize) -> NodeId {
        let point = self.name_start();
        let t = if self.tok() == Token::This {
            self.aux_constructor(mods, num_lead)
        } else {
            self.method_def(mods, num_lead)
        };
        self.at_span2(start, point, t)
    }

    fn method_def(&mut self, mods: Modifiers, num_lead: usize) -> NodeId {
        let mods1 = self.add_flag(mods, flags::METHOD);
        let ident = self.term_ident();
        let Tree::Ident { name } = *self.ast.tree(ident) else { unreachable!("an identifier") };
        let paramss = if self.s.cfg.source_version.enables_clause_interleaving() {
            self.type_or_term_param_clauses(ParamOwner::Def, num_lead)
        } else {
            let tparams = self.type_param_clause_opt(ParamOwner::Def);
            let vparamss = self.term_param_clauses(ParamOwner::Def, num_lead);
            self.join_params(&tparams, vparamss)
        };
        let saved = std::mem::replace(&mut self.in_fun_return_type, true);
        let mut tpt = self.typed_opt();
        self.in_fun_return_type = saved;
        if self.s.cfg.source_version.migrate_to_3() {
            self.new_line_opt_when_followed_by(Token::LBrace);
        }
        let rhs = if self.tok() == Token::Equals {
            self.next();
            Some(self.sub_expr())
        } else if !self.is_empty(tpt) {
            None
        } else if self.procedure_syntax(": Unit") {
            let unit = self.type_name("Unit");
            tpt = self.scala_dot(unit);
            if self.tok() == Token::LBrace { Some(self.expr()) } else { None }
        } else if self.tok() == Token::Identifier
            && paramss.is_empty()
            && self.s.names.text(name.name).ends_with(':')
        {
            let help = self.colon_name_help(ident);
            self.accept_help(Token::Equals, &help);
            None
        } else {
            if !self.is_expr_intro() {
                let l = self.last_offset();
                self.syntax_error(Message::syntax(19, "Missing return type"), l);
            }
            self.accept(Token::Equals);
            Some(self.expr())
        };
        let paramss = self.ast.clauses(&paramss);
        let m = self.empty_mods();
        let d = self.ast.node(Tree::DefDef { name, paramss, tpt, rhs, mods: m });
        if self.ast.get(ident).attrs.contains(Attrs::BACKQUOTED) {
            self.ast.add_attrs(d, Attrs::BACKQUOTED);
        }
        self.finalize_def(d, mods1)
    }

    /// `tparams :: vparamss`, without an empty type parameter clause (dotty's `joinParams`).
    pub fn join_params(&mut self, tparams: &[NodeId], vparamss: Vec<List>) -> Vec<List> {
        if tparams.is_empty() {
            return vparamss;
        }
        let tps = self.ast.list(tparams);
        std::iter::once(tps).chain(vparamss).collect()
    }

    fn aux_constructor(&mut self, mods: Modifiers, num_lead: usize) -> NodeId {
        self.next();
        let vparamss = self.term_param_clauses(ParamOwner::Def, num_lead);
        let bad_first = vparamss.first().is_none_or(|&c| {
            self.ast.items(c).first().is_some_and(|&p| match self.ast.tree(p) {
                Tree::ValDef { mods, .. } => self.ast.modifiers(*mods).is_one_of(flags::GIVEN_OR_IMPLICIT),
                _ => false,
            })
        });
        if bad_first {
            let msg = Message::syntax(26, "Auxiliary constructor needs non-implicit parameter list");
            match self.tok() {
                Token::LBracket => {
                    let o = self.offset();
                    self.syntax_error("no type parameters allowed here", o);
                }
                Token::Eof => self.incomplete_input_error(msg),
                _ => {
                    let o = self.name_start();
                    self.syntax_error(msg, o);
                }
            }
        }
        if self.s.cfg.source_version.migrate_to_3() {
            self.new_line_opt_when_followed_by(Token::LBrace);
        }
        if !(self.tok() == Token::LBrace && self.procedure_syntax("")) {
            self.accept(Token::Equals);
        }
        let rhs_start = self.offset();
        let rhs = self.sub_part(|p| p.constr_expr());
        let rhs = self.at_span1(rhs_start, rhs);
        let c = self.make_constructor(&vparamss, Some(rhs));
        let id = self.ast.mods(mods);
        self.set_mods(c, id);
        c
    }

    /// Procedure syntax `def f() {...}`: accepted only under 3.0-migration.
    fn procedure_syntax(&mut self, result_type: &str) -> bool {
        if !self.s.cfg.source_version.migrate_to_3() {
            return false;
        }
        let insert =
            if self.tok() == Token::LBrace { format!("{result_type} =") } else { ": Unit ".to_string() };
        let msg = format!(
            "Procedure syntax no longer supported; `{insert}` should be inserted here{}",
            rewrite_notice("This construct", "3.0-migration", "")
        );
        self.migration_error(msg, self.offset(), Migration::Scala2to3);
        true
    }

    /// `SelfInvocation` or `{ SelfInvocation {semi BlockStat} }`.
    fn constr_expr(&mut self) -> NodeId {
        if self.s.cur.is_nested_start() {
            let start = self.offset();
            let b = self.in_braces_or_indented(|p| {
                let mut stats = vec![p.self_invocation()];
                if p.s.cur.is_stat_sep() {
                    p.next();
                    stats.extend(p.block_stat_seq());
                }
                p.unit_block(&stats)
            });
            self.at_span1(start, b)
        } else {
            let inv = self.self_invocation();
            self.unit_block(&[inv])
        }
    }

    fn unit_block(&mut self, stats: &[NodeId]) -> NodeId {
        let stats = self.ast.list(stats);
        let unit = self.ast.node(Tree::Literal { value: Constant::Unit });
        self.ast.add_attrs(unit, Attrs::SYNTHETIC_UNIT);
        self.ast.node(Tree::Block { stats, expr: Some(unit) })
    }

    fn self_invocation(&mut self) -> NodeId {
        let start = self.accept(Token::This);
        self.argument_start();
        let init = self.term("<init>");
        let fun = self.ast.node(Tree::Ident { name: init });
        let app = self.argument_apply(fun);
        let t = self.argument_exprss(app);
        self.at_span1(start, t)
    }
}
