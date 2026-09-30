//! The parameters of a lambda whose parameter part was parsed as an expression.

use super::super::Parser;
use super::super::migration::Migration;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, TName, Tree};
use crate::diag::rewrite_notice;
use crate::token::Token;

impl Parser<'_> {
    /// The parameters of a lambda whose parameter part was parsed as expression `t`.
    pub fn convert_to_params(&mut self, t: NodeId) -> Vec<NodeId> {
        let given = self.tok() == Token::CtxArrow || self.is_pure_arrow_named("?->");
        let mods = if given { Modifiers::with_flags(flags::GIVEN) } else { Modifiers::default() };
        match self.ast.tree(t).clone() {
            Tree::Parens { t } => vec![self.convert_to_param(t, mods)],
            Tree::Tuple { trees } => {
                let ts = self.ast.items(trees).to_vec();
                ts.into_iter().map(|t| self.convert_to_param(t, mods.clone())).collect()
            }
            Tree::Typed { expr, .. } if matches!(self.ast.tree(expr), Tree::Ident { .. }) => {
                let msg = format!(
                    "parentheses are required around the parameter of a lambda{}",
                    rewrite_notice("This construct", "3.0-migration", "")
                );
                self.migration_error(msg, self.offset(), Migration::Scala2to3);
                vec![self.convert_to_param(t, mods)]
            }
            _ => vec![self.convert_to_param(t, mods)],
        }
    }

    fn convert_to_param(&mut self, t: NodeId, mods: Modifiers) -> NodeId {
        let span = self.ast.span(t);
        let backquoted = |p: &Self, id: NodeId| p.ast.get(id).attrs.contains(Attrs::BACKQUOTED);
        match self.ast.tree(t).clone() {
            Tree::ValDef { mods: m, .. } => {
                let m = self.ast.modifiers_mut(m);
                m.flags = m.flags.or(mods.flags);
                t
            }
            Tree::Ident { name } => {
                let bq = backquoted(self, t);
                let tpt = self.type_tree();
                let p = self.make_parameter(TName { is_type: false, ..name }, tpt, mods, bq);
                self.ast.with_span(p, span);
                p
            }
            Tree::Typed { expr, tpt } if !matches!(self.ast.tree(tpt), Tree::TypeBoundsTree { .. }) => {
                match *self.ast.tree(expr) {
                    Tree::Ident { name } => {
                        let bq = backquoted(self, expr);
                        let p = self.make_parameter(TName { is_type: false, ..name }, tpt, mods, bq);
                        self.ast.with_span(p, span);
                        p
                    }
                    _ => self.not_a_parameter(t, mods),
                }
            }
            _ => self.not_a_parameter(t, mods),
        }
    }

    fn not_a_parameter(&mut self, t: NodeId, mods: Modifiers) -> NodeId {
        let span = self.ast.span(t);
        self.syntax_error_span("not a legal formal parameter for a function literal", span);
        let name = self.term("<error>");
        self.make_parameter(name, t, mods, false)
    }

    /// Report a parameter in a tuple that is not a lambda's parameter list.
    pub fn check_non_param_tuple(&mut self, t: NodeId) {
        if let Tree::Tuple { trees } = *self.ast.tree(t)
            && let Some(&p) =
                self.ast.items(trees).iter().find(|&&x| matches!(self.ast.tree(x), Tree::ValDef { .. }))
        {
            let span = self.ast.span(p);
            self.syntax_error_span("invalid parameter definition syntax in tuple value", span);
        }
    }
}
