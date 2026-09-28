use super::ParamOwner;
use crate::ast::flags;
use crate::ast::mods::{Mod, Modifiers};
use crate::ast::{List, NodeId, TName, Tree};
use crate::parse::Parser;
use crate::parse::migration::Migration;
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// A given definition after `given`, old or new syntax.
    pub fn given_def(&mut self, start: Offset, mods: Modifiers, given_mod: Mod) -> NodeId {
        let point = self.name_start();
        let mut mods1 = self.add_mod(mods.clone(), given_mod);
        let mut new_syntax = self.s.cfg.source_version.enables_new_givens();
        let embedded_colon = !self.s.cur.is_colon() && self.following_is_given_def_with_colon();
        let name = if self.is_ident() && embedded_colon { self.ident() } else { self.term("") };
        let name = TName { is_type: false, ..name };
        let unnamed = self.s.names.text(name.name).is_empty();
        let (tparams, vparamss0, parents) = if self.s.cur.is_colon() && !unnamed {
            self.next();
            self.new_signature(new_syntax)
        } else if embedded_colon {
            let msg = "This old given syntax is no longer supported; use `=>` instead of `:`";
            self.migration_error(msg, self.offset(), Migration::GivenSyntax);
            new_syntax = false;
            let tparams = self.type_param_clause_opt(ParamOwner::Given);
            self.new_line_opt();
            let la = self.s.lookahead();
            let using = la.is_ident() && la.name.is_some_and(|n| self.s.names.text(n) == "using");
            let vparamss = if self.tok() == Token::LParen && (using || !unnamed) {
                self.term_param_clauses(ParamOwner::Given, 0)
            } else {
                Vec::new()
            };
            self.accept_colon();
            let parents = self.implemented(new_syntax);
            (tparams, vparamss, parents)
        } else {
            self.new_signature(new_syntax)
        };
        let has_params = !tparams.is_empty() || !vparamss0.is_empty();
        let vparamss = match vparamss0[..] {
            [only] if only.is_empty() => Vec::new(),
            _ => vparamss0,
        };
        let parents_is_type = parents.len() == 1 && self.is_type(parents[0]);
        let at_end = self.s.cur.is_stat_sep() || self.is_stat_seq_end();
        let gdef = if self.tok() == Token::Equals && parents_is_type {
            self.accept(Token::Equals);
            mods1 = mods1.or(flags::FINAL);
            if !has_params && !mods.is(flags::INLINE) {
                if !mods.is(flags::ERASED) {
                    mods1 = mods1.or(flags::LAZY);
                }
                let rhs = self.sub_expr();
                let m = self.empty_mods();
                self.ast.node(Tree::ValDef { name, tpt: parents[0], rhs: Some(rhs), mods: m })
            } else {
                let rhs = self.sub_expr();
                self.given_method(name, &tparams, vparamss, parents[0], Some(rhs))
            }
        } else if at_end && parents_is_type && !(unnamed && new_syntax) {
            if unnamed {
                let msg = "Anonymous given cannot be abstract, or maybe you want to define a concrete given and are missing a `()` argument?";
                let l = self.last_offset();
                self.syntax_error(msg, l);
            }
            if new_syntax {
                let msg = "This defines an abstract given, which is no longer supported. Use a `deferred` given instead.\nOr, if you intend to define a concrete given, follow the type with `()` arguments.";
                let l = self.last_offset();
                self.migration_error(msg, l, Migration::GivenSyntax);
            }
            self.given_method(name, &tparams, vparamss, parents[0], None)
        } else {
            for &c in &vparamss {
                for &p in self.ast.items(c).to_vec().iter() {
                    self.update_param_mods(p, |m| {
                        if m.is(flags::PRIVATE) {
                            m.clone().and_not(flags::PRIVATE_LOCAL).or(flags::PROTECTED)
                        } else {
                            m.clone()
                        }
                    });
                }
            }
            let empty_params = tparams.is_empty() && vparamss.is_empty();
            let paramss = self.join_params(&tparams, vparamss);
            let constr = self.make_constructor(&paramss, None);
            let templ = if at_end {
                self.make_template(constr, &parents, &[], None, &[])
            } else if !new_syntax || self.tok() == Token::With && empty_params {
                self.with_template(constr, &parents)
            } else {
                self.new_template_start();
                self.template_body_opt(constr, &parents, &[])
            };
            let m = self.empty_mods();
            if !has_params && !mods.is(flags::INLINE) {
                self.ast.node(Tree::ModuleDef { name, imp: templ, mods: m })
            } else {
                self.ast.node(Tree::TypeDef { name: TName { is_type: true, ..name }, rhs: templ, mods: m })
            }
        };
        let gdef = self.finalize_def(gdef, mods1);
        self.at_span2(start, point, gdef)
    }

    fn given_method(
        &mut self,
        name: TName,
        tparams: &[NodeId],
        vparamss: Vec<List>,
        tpt: NodeId,
        rhs: Option<NodeId>,
    ) -> NodeId {
        let paramss = self.join_params(tparams, vparamss);
        for &c in &paramss {
            for &p in self.ast.items(c).to_vec().iter() {
                self.adjust_def_param(p);
            }
        }
        let paramss = self.ast.clauses(&paramss);
        let m = self.empty_mods();
        self.ast.node(Tree::DefDef { name, paramss, tpt, rhs, mods: m })
    }

    /// Turn a class parameter of a given into a method parameter.
    fn adjust_def_param(&mut self, p: NodeId) {
        let Some(m) = self.def_mods(p) else { return };
        if !m.flags.is_all_of(flags::PRIVATE_LOCAL) {
            let name = match self.ast.tree(p) {
                Tree::ValDef { name, .. } | Tree::TypeDef { name, .. } => {
                    self.s.names.text(name.name).to_string()
                }
                _ => String::new(),
            };
            let span = self.ast.span(p);
            self.syntax_error_span(format!("method parameter {name} may not be a `val`"), span);
        }
        let dropped = flags::ACCESS_FLAGS.or(flags::PARAM_ACCESSOR).or(flags::MUTABLE);
        self.update_param_mods(p, |m| m.clone().and_not(dropped).or(flags::PARAM));
    }

    fn def_mods(&self, p: NodeId) -> Option<Modifiers> {
        match *self.ast.tree(p) {
            Tree::ValDef { mods, .. } | Tree::TypeDef { mods, .. } => Some(self.ast.modifiers(mods).clone()),
            _ => None,
        }
    }

    fn update_param_mods(&mut self, p: NodeId, f: impl FnOnce(&Modifiers) -> Modifiers) {
        let Some(m) = self.def_mods(p) else { return };
        let id = self.ast.mods(f(&m));
        match &mut self.ast.get_mut(p).tree {
            Tree::ValDef { mods, .. } | Tree::TypeDef { mods, .. } => *mods = id,
            _ => {}
        }
    }
}
