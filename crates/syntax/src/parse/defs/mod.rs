//! Definitions: `val`, `var`, `def`, `type`, templates, givens and extensions.

mod context_params;
mod defdef;
mod extension;
mod given;
mod given_sig;
mod params;
mod term_param;
mod tmpl;
mod type_params;
mod typedef;
mod valdef;

use super::Parser;
use crate::ast::modifiers::{Mod, ModKind, Modifiers};
use crate::ast::{Attrs, NodeId, Span, Tree};
use crate::config::SourceVersion;
use crate::diag::Message;
use crate::source::Offset;
use crate::token::Token;

/// What owns the parameters being parsed (dotty's `ParamOwner`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamOwner {
    Class,
    CaseClass,
    Def,
    Type,
    Hk,
    Given,
    ExtensionPrefix,
    ExtensionFollow,
}

impl ParamOwner {
    pub fn is_class(self) -> bool {
        matches!(self, ParamOwner::Class | ParamOwner::CaseClass | ParamOwner::Given)
    }

    pub fn takes_only_using_clauses(self) -> bool {
        matches!(self, ParamOwner::Given | ParamOwner::ExtensionFollow)
    }

    pub fn accepts_variance(self) -> bool {
        matches!(self, ParamOwner::Class | ParamOwner::CaseClass | ParamOwner::Hk)
    }

    pub fn accepts_ctx_bounds(self, v: SourceVersion) -> bool {
        !matches!(self, ParamOwner::Type | ParamOwner::Hk)
            || v.enables_new_givens() && self == ParamOwner::Type
    }

    pub fn accepts_wildcard(self) -> bool {
        matches!(self, ParamOwner::Type | ParamOwner::Hk)
    }
}

impl Parser<'_> {
    /// Give definition `md` its modifiers; warn about a template name ending in `:`.
    pub fn finalize_def(&mut self, md: NodeId, mods: Modifiers) -> NodeId {
        let backquoted = self.ast.get(md).attrs.contains(Attrs::BACKQUOTED);
        let empty_body =
            |p: &Self, t: NodeId| matches!(p.ast.tree(t), Tree::Template { body, .. } if body.is_empty());
        let checked = match *self.ast.tree(md) {
            Tree::TypeDef { name, rhs, .. } if empty_body(self, rhs) && !backquoted => Some(name),
            Tree::ModuleDef { name, imp, .. } if empty_body(self, imp) && !backquoted => Some(name),
            _ => None,
        };
        if let Some(name) = checked {
            let text = self.s.names.text(name.name);
            if !text.is_empty() && !text.starts_with(crate::scan::is_operator_part) && text.ends_with(':') {
                let msg = Message::syntax(228, format!("name `{text}` should be enclosed in backticks"));
                let point = self.ast.span(md).point();
                self.warning(msg, point);
            }
        }
        let id = self.ast.mods(mods);
        self.set_mods(md, id);
        md
    }

    pub fn set_mods(&mut self, md: NodeId, id: crate::ast::tree::ModsId) {
        match &mut self.ast.get_mut(md).tree {
            Tree::ValDef { mods, .. }
            | Tree::DefDef { mods, .. }
            | Tree::TypeDef { mods, .. }
            | Tree::ModuleDef { mods, .. }
            | Tree::PatDef { mods, .. } => *mods = id,
            _ => {}
        }
    }

    /// Span `t` from `start` with its point at the name at the current token, marking it
    /// backquoted if the name is (dotty's `atNameSpan`).
    pub fn at_name_span(&mut self, start: Offset, body: impl FnOnce(&mut Self) -> NodeId) -> NodeId {
        let backquoted = self.tok() == Token::BackquotedIdent;
        let point = self.name_start();
        let t = body(self);
        let t = self.at_span2(start, point, t);
        if backquoted {
            self.ast.add_attrs(t, Attrs::BACKQUOTED);
        }
        t
    }

    /// `val`, `var`, `def`, `type`, an enum case, or a template definition.
    pub fn def_or_dcl(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        match self.tok() {
            Token::Val => {
                self.next();
                self.pat_def_or_dcl(start, mods)
            }
            Token::Var => {
                let s = self.s.skip_token();
                let m = Mod { kind: ModKind::Var, span: Span::new(s, self.last_offset(), s) };
                let mods = self.add_mod(mods, m);
                self.pat_def_or_dcl(start, mods)
            }
            Token::Def => {
                self.next();
                self.def_def_or_dcl(start, mods, 0)
            }
            Token::Type => {
                self.next();
                self.type_def_or_dcl(start, mods)
            }
            Token::Case if self.in_enum => self.enum_case(start, mods),
            _ => self.tmpl_def(start, mods),
        }
    }
}
