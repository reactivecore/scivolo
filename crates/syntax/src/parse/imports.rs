// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! `import` and `export` clauses.

use super::Parser;
use super::migration::Migration;
use crate::ast::{NodeId, Span, Tree};
use crate::diag::rewrite_notice;
use crate::token::Token;
use crate::token::sets::CAN_START_INFIX_TYPE_TOKENS;

impl Parser<'_> {
    pub fn import_clause(&mut self, outermost: bool) -> Vec<NodeId> {
        self.import_or_export_clause(false, outermost)
    }

    pub fn export_clause(&mut self) -> Vec<NodeId> {
        self.import_or_export_clause(true, false)
    }

    fn import_or_export_clause(&mut self, export: bool, outermost: bool) -> Vec<NodeId> {
        let offset = self.accept(if export { Token::Export } else { Token::Import });
        let ts = self.comma_separated(|p| {
            let start = p.offset();
            let qual = p.simple_ref();
            let t = p.import_selection(qual, export, outermost);
            p.at_span1(start, t)
        });
        if let Some(&first) = ts.first() {
            let span = self.ast.span(first);
            let span = if span.exists() {
                span.with_start(offset)
            } else {
                Span::synthetic(offset, self.last_offset())
            };
            self.ast.with_span(first, span);
        }
        ts
    }

    fn import_selection(&mut self, qual: NodeId, export: bool, outermost: bool) -> NodeId {
        if self.is_ident_named("as") {
            match *self.ast.tree(qual) {
                Tree::Select { qualifier, name } => {
                    let qs = self.ast.span(qual);
                    let from = self.ast.node(Tree::Ident { name });
                    self.ast.with_span(from, Span::new(qs.point(), qs.end(), qs.point()));
                    let sel = self.named_selector(from, export);
                    return self.mk_import(qualifier, &[sel], export, outermost);
                }
                Tree::Ident { .. } => {
                    let sel = self.named_selector(qual, export);
                    let empty = self.empty_tree();
                    return self.mk_import(empty, &[sel], export, outermost);
                }
                _ => {}
            }
        }
        self.accept(Token::Dot);
        let sels = match self.tok() {
            Token::Uscore => vec![self.wildcard_selector(export)],
            Token::Given => vec![self.given_selector()],
            Token::LBrace => self.in_braces(|p| p.import_selectors(export)),
            _ if self.is_ident_named("*") => vec![self.wildcard_selector(export)],
            _ => {
                let start = self.offset();
                let name = self.ident();
                if self.tok() == Token::Dot {
                    let qs = self.start_offset(qual);
                    let sel = self.ast.node(Tree::Select { qualifier: qual, name });
                    let sel = self.at_span2(qs, start, sel);
                    return self.import_selection(sel, export, outermost);
                }
                let id = self.ast.node(Tree::Ident { name });
                let id = self.at_span1(start, id);
                vec![self.named_selector(id, export)]
            }
        };
        self.mk_import(qual, &sels, export, outermost)
    }

    fn import_selector(
        &mut self,
        imported: NodeId,
        renamed: Option<NodeId>,
        bound: Option<NodeId>,
    ) -> NodeId {
        self.ast.node(Tree::ImportSelector { imported, renamed, bound })
    }

    /// `*` or `_`.
    fn wildcard_selector(&mut self, export: bool) -> NodeId {
        if self.tok() == Token::Uscore {
            let what = if export { "export" } else { "import" };
            let msg = format!(
                "`_` is no longer supported for a wildcard {what}; use `*` instead{}",
                rewrite_notice("This construct", "future-migration", "")
            );
            self.migration_error(msg, self.offset(), Migration::ImportWildcard);
        }
        let start = self.s.skip_token();
        let name = self.term("_");
        let id = self.ast.node(Tree::Ident { name });
        let id = self.at_span1(start, id);
        self.import_selector(id, None, None)
    }

    /// `given [InfixType]`.
    fn given_selector(&mut self) -> NodeId {
        let start = self.s.skip_token();
        let name = self.term("");
        let id = self.ast.node(Tree::Ident { name });
        let id = self.at_span1(start, id);
        let bound = if CAN_START_INFIX_TYPE_TOKENS.contains(self.tok()) {
            let t = self.infix_type();
            Some(self.reject_wildcard_type(t))
        } else {
            None
        };
        self.import_selector(id, None, bound)
    }

    /// `id [as (id | _)]`.
    fn named_selector(&mut self, from: NodeId, export: bool) -> NodeId {
        if !(self.tok() == Token::Arrow || self.is_ident_named("as")) {
            return self.import_selector(from, None, None);
        }
        if self.tok() == Token::Arrow {
            let what = if export { "export" } else { "import" };
            let msg = format!(
                "The {what} renaming `a => b` is no longer supported ; use `a as b` instead{}",
                rewrite_notice("This construct", "future-migration", "")
            );
            self.migration_error(msg, self.offset(), Migration::ImportRename);
        }
        let start = self.start_offset(from);
        let point = self.s.skip_token();
        let to = if self.tok() == Token::Uscore { self.wildcard_ident() } else { self.term_ident() };
        let renamed = match *self.ast.tree(to) {
            Tree::Ident { name } if self.s.names.text(name.name) == "<error>" => None,
            _ => Some(to),
        };
        let s = self.import_selector(from, renamed, None);
        self.at_span2(start, point, s)
    }

    fn import_selectors(&mut self, export: bool) -> Vec<NodeId> {
        let mut id_ok = true;
        self.comma_separated(|p| {
            let wildcard = matches!(p.tok(), Token::Uscore | Token::Given) || p.is_ident_named("*");
            let start = p.offset();
            let sel = match p.tok() {
                Token::Uscore => p.wildcard_selector(export),
                Token::Given => p.given_selector(),
                _ if p.is_ident_named("*") => p.wildcard_selector(export),
                _ => {
                    if !id_ok {
                        let what = if export { "export" } else { "import" };
                        let o = p.offset();
                        p.syntax_error(format!("named {what}s cannot follow wildcard {what}s"), o);
                    }
                    let id = p.term_ident();
                    p.named_selector(id, export)
                }
            };
            id_ok &= !wildcard;
            p.at_span1(start, sel)
        })
    }

    fn mk_import(&mut self, expr: NodeId, sels: &[NodeId], export: bool, outermost: bool) -> NodeId {
        let selectors = self.ast.list(sels);
        if export {
            return self.ast.node(Tree::Export { expr, selectors, ext: None });
        }
        let imp = self.ast.node(Tree::Import { expr, selectors });
        if let Some(prefix) = self.language_prefix(expr) {
            self.language_import(&prefix, sels, outermost);
        }
        imp
    }
}
