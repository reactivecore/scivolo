use super::ParamOwner;
use crate::ast::flags;
use crate::ast::modifiers::{Mod, ModKind, Modifiers};
use crate::ast::{NodeId, Span, TName, Tree};
use crate::parse::Parser;
use crate::source::Offset;
use crate::token::Token;
use crate::token::sets::ACCESS_MODIFIER_TOKENS;

impl Parser<'_> {
    /// A class, trait, object, enum or given definition.
    pub fn tmpl_def(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        self.nested(|p| p.tmpl_def_at_level(start, mods))
    }

    fn tmpl_def_at_level(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        match self.tok() {
            Token::Trait => {
                let mods = self.add_flag(mods, flags::TRAIT);
                self.next();
                self.class_def(start, mods)
            }
            Token::Class => {
                self.next();
                self.class_def(start, mods)
            }
            Token::CaseClass => {
                self.next();
                self.class_def(start, mods.or(flags::CASE))
            }
            Token::Object => {
                self.next();
                self.object_def(start, mods.or(flags::MODULE))
            }
            Token::CaseObject => {
                self.next();
                self.object_def(start, mods.or(flags::CASE.or(flags::MODULE)))
            }
            Token::Enum => {
                self.next();
                self.enum_def(start, mods.or(flags::ENUM))
            }
            Token::Given => {
                let s = self.s.skip_token();
                let m = Mod { kind: ModKind::Given, span: Span::new(s, self.last_offset(), s) };
                self.given_def(start, mods, m)
            }
            _ => {
                let start = self.last_offset();
                let msg = crate::diag::Message::syntax(88, "Expected start of definition");
                self.syntax_error_or_incomplete(msg, self.offset());
                match *self.ast.items(mods.annotations) {
                    [head] => head,
                    [] => self.empty_tree(),
                    _ => {
                        let expr = self.error_term_tree(start);
                        self.ast.node(Tree::Block { stats: mods.annotations, expr: Some(expr) })
                    }
                }
            }
        }
    }

    /// `id ClassConstr TemplateOpt`.
    fn class_def(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        let owner = if mods.is(flags::CASE) { ParamOwner::CaseClass } else { ParamOwner::Class };
        let td = self.at_name_span(start, |p| {
            let name = p.ident();
            let constr = p.class_constr(owner);
            let templ = p.template_opt(constr);
            let m = p.empty_mods();
            p.ast.node(Tree::TypeDef { name: TName { is_type: true, ..name }, rhs: templ, mods: m })
        });
        self.finalize_def(td, mods)
    }

    /// `[ClsTypeParamClause] [ConstrMods] ClsTermParamClauses`.
    pub fn class_constr(&mut self, owner: ParamOwner) -> NodeId {
        let start = self.last_offset();
        let tparams = self.type_param_clause_opt(owner);
        let saved = std::mem::replace(&mut self.in_class_constr_annots, true);
        let annots = self.annots_as_mods(false);
        let cmods = self.modifiers(ACCESS_MODIFIER_TOKENS, annots);
        self.in_class_constr_annots = saved;
        let vparamss = self.term_param_clauses(owner, 0);
        let paramss = self.join_params(&tparams, vparamss);
        let c = self.make_constructor(&paramss, None);
        let id = self.ast.mods(cmods);
        self.set_mods(c, id);
        self.at_span1(start, c)
    }

    /// `id TemplateOpt`.
    pub fn object_def(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        let md = self.at_name_span(start, |p| {
            let id = p.term_ident();
            let Tree::Ident { name } = *p.ast.tree(id) else { unreachable!("an identifier") };
            let constr = p.empty_constructor();
            let templ = p.template_opt(constr);
            let m = p.empty_mods();
            p.ast.node(Tree::ModuleDef { name: TName { is_type: false, ..name }, imp: templ, mods: m })
        });
        self.finalize_def(md, mods)
    }

    /// `mods` without the modifiers an enum (case) does not allow, reporting each.
    fn check_enum_modifiers(&mut self, mods: Modifiers, case_str: &str) -> Modifiers {
        let allowed = flags::ACCESS_FLAGS.or(flags::ENUM).or(flags::INFIX).or(flags::INTO);
        let mut f = mods.flags;
        for m in mods.mods.clone() {
            if !m.kind.flags().is_one_of(allowed) {
                self.syntax_error_span(format!("This modifier is not allowed on an enum{case_str}"), m.span);
                f = f.and_not(m.kind.flags());
            }
        }
        Modifiers { flags: f, ..mods }
    }

    /// `id ClassConstr InheritClauses EnumBody`.
    fn enum_def(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        self.at_name_span(start, |p| {
            let mods1 = p.check_enum_modifiers(mods, "");
            let name = p.ident();
            let constr = p.class_constr(ParamOwner::Class);
            let templ = p.template(constr, true);
            let m = p.empty_mods();
            let td = p.ast.node(Tree::TypeDef { name: TName { is_type: true, ..name }, rhs: templ, mods: m });
            p.finalize_def(td, mods1)
        })
    }

    /// `case id ClassConstr [extends ConstrApps]` or `case ids`.
    pub fn enum_case(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        let mods1 = self.check_enum_modifiers(mods, " case").or(flags::ENUM_CASE);
        self.accept(Token::Case);
        let point = self.name_start();
        let id = self.term_ident();
        let Tree::Ident { name } = *self.ast.tree(id) else { unreachable!("an identifier") };
        let t = if self.tok() == Token::Comma {
            self.next();
            let mut ids = vec![id];
            ids.extend(self.comma_separated(|p| p.term_ident()));
            let pats = self.ast.list(&ids);
            let tpt = self.type_tree();
            let m = self.ast.mods(mods1);
            self.ast.node(Tree::PatDef { mods: m, pats, tpt, rhs: None })
        } else {
            let m = self.empty_mods();
            let case_def = if matches!(self.tok(), Token::LBracket | Token::LParen | Token::At)
                || self.is_modifier()
            {
                let constr = self.class_constr(ParamOwner::CaseClass);
                let templ = self.case_template(constr);
                self.ast.node(Tree::TypeDef { name: TName { is_type: true, ..name }, rhs: templ, mods: m })
            } else {
                let constr = self.empty_constructor();
                let templ = self.case_template(constr);
                self.ast.node(Tree::ModuleDef { name, imp: templ, mods: m })
            };
            self.finalize_def(case_def, mods1)
        };
        self.at_span2(start, point, t)
    }

    /// `[extends ConstrApps]`.
    fn case_template(&mut self, constr: NodeId) -> NodeId {
        let parents = if self.tok() == Token::Extends {
            self.next();
            self.constr_apps(None)
        } else {
            Vec::new()
        };
        self.make_template(constr, &parents, &[], None, &[])
    }
}
