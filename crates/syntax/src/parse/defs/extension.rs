// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
use super::ParamOwner;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, Tree};
use crate::parse::Parser;
use crate::token::Token;
use crate::token::sets::MODIFIER_TOKENS;

impl Parser<'_> {
    /// `extension [DefTypeParamClause] {UsingParamClause} (DefTermParam) {UsingParamClause} ExtMethods`.
    pub fn extension(&mut self) -> NodeId {
        let start = self.s.skip_token();
        let tparams = self.type_param_clause_opt(ParamOwner::ExtensionPrefix);
        let mut lead = Vec::new();
        let mut num_lead = 0;
        loop {
            let ps =
                self.term_param_clause(ParamOwner::ExtensionPrefix, num_lead, false, Modifiers::default());
            num_lead += ps.len();
            let using = ps.first().is_some_and(|&p| match self.ast.tree(p) {
                Tree::ValDef { mods, .. } => self.ast.modifiers(*mods).is(flags::GIVEN),
                _ => false,
            });
            lead.push(self.ast.list(&ps));
            if !using {
                break;
            }
        }
        let follow = self.term_param_clauses(ParamOwner::ExtensionFollow, num_lead);
        lead.extend(follow.into_iter().filter(|c| !c.is_empty()));
        let num_lead = lead.iter().map(|c| c.len()).sum();
        if self.s.cur.is_colon() {
            let o = self.offset();
            self.syntax_error("no `:` expected here", o);
            self.next();
        }
        let methods = if self.tok() == Token::Export {
            self.export_clause()
        } else if self.is_def_intro(MODIFIER_TOKENS, false) {
            vec![self.ext_method(num_lead)]
        } else {
            self.s.observe_indented();
            self.new_line_opt_when_followed_by(Token::LBrace);
            if self.s.cur.is_nested_start() {
                self.in_braces_or_indented(|p| p.ext_methods(num_lead))
            } else {
                self.syntax_error_or_incomplete("Extension without extension methods", self.offset());
                Vec::new()
            }
        };
        let paramss = self.join_params(&tparams, lead);
        let paramss = self.ast.clauses(&paramss);
        let methods = self.ast.list(&methods);
        let t = self.ast.node(Tree::ExtMethods { paramss, methods });
        self.at_span1(start, t)
    }

    /// `{Annotation [nl]} {Modifier} def DefDef`.
    fn ext_method(&mut self, num_lead: usize) -> NodeId {
        let start = self.offset();
        let mods = self.def_annots_mods(MODIFIER_TOKENS);
        self.accept(Token::Def);
        self.def_def_or_dcl(start, mods, num_lead)
    }

    /// `ExtMethod {semi ExtMethod}`, reporting other definitions.
    fn ext_methods(&mut self, num_lead: usize) -> Vec<NodeId> {
        self.check_no_escaping_placeholders(|p| {
            let mut meths = Vec::new();
            loop {
                let start = p.offset();
                let mods = p.def_annots_mods(MODIFIER_TOKENS);
                match p.tok() {
                    Token::Import => {
                        let o = p.offset();
                        p.syntax_error("imports are not allowed inside extensions", o);
                        meths.extend(p.import_clause(false));
                    }
                    Token::Export => meths.extend(p.export_clause()),
                    Token::Val | Token::Var => {
                        p.next();
                        let t = p.pat_def_or_dcl(start, mods);
                        meths.push(t);
                        let span = p.ast.span(t);
                        p.syntax_error_span("values are not allowed inside extensions", span);
                    }
                    Token::Def => {
                        p.next();
                        meths.push(p.def_def_or_dcl(start, mods, num_lead));
                    }
                    Token::Type => {
                        p.next();
                        let t = p.type_def_or_dcl(start, mods);
                        meths.push(t);
                        let span = p.ast.span(t);
                        p.syntax_error_span("types are not allowed inside extensions", span);
                    }
                    _ => {}
                }
                if p.tok() == Token::Eof
                    || !p.stat_sep_or_end(&mut meths, false, "extension method", Token::Eof)
                {
                    break;
                }
            }
            if meths.is_empty() {
                p.syntax_error_or_incomplete("`def` expected", p.offset());
            }
            meths
        })
    }
}
