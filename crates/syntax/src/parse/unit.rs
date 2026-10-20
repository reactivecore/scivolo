// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Compilation units, packagings and top-level statements.

use super::Parser;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, Tree};
use crate::source::Offset;
use crate::token::Token;
use crate::token::sets::MODIFIER_TOKENS;

impl Parser<'_> {
    /// Parse the whole source.
    pub fn parse(&mut self) -> NodeId {
        let t = self.compilation_unit();
        self.accept(Token::Eof);
        t
    }

    /// Parse the whole source as block statements, as dotty's REPL does (`ParseResult.parseStats`).
    /// On a `SourceFile::maybe_incomplete` source, `s.diags.is_incomplete()` then tells whether the
    /// input ended where more was expected (`ParseResult.isIncomplete`); the trees and errors to
    /// report come from a parse of a `SourceFile::new` source.
    pub fn parse_stats(&mut self) -> Vec<NodeId> {
        let stats = self.block_stat_seq();
        self.accept(Token::Eof);
        stats
    }

    /// `{package QualId semi} TopStatSeq`.
    fn compilation_unit(&mut self) -> NodeId {
        let stats = self.check_no_escaping_placeholders(|p| p.top_stats());
        match stats[..] {
            [stat] if matches!(self.ast.tree(stat), Tree::PackageDef { .. }) => stat,
            [] => self.empty_tree(),
            _ => {
                let name = self.term("<empty>");
                let pid = self.ast.node(Tree::Ident { name });
                let stats = self.ast.list(&stats);
                self.ast.node(Tree::PackageDef { pid, stats })
            }
        }
    }

    fn top_stats(&mut self) -> Vec<NodeId> {
        let mut ts = Vec::new();
        while self.tok() == Token::Semi {
            self.next();
        }
        let start = self.offset();
        if self.tok() != Token::Package {
            ts.extend(self.top_stat_seq(true));
            return ts;
        }
        self.next();
        let more = if self.tok() == Token::Object {
            self.next();
            ts.push(self.object_def(start, Modifiers::with_flags(flags::PACKAGE)));
            self.tok() != Token::Eof
        } else {
            let pkg = self.qual_id();
            self.new_template_start();
            if self.tok() == Token::Eof {
                ts.push(self.make_packaging(start, pkg, &[]));
                false
            } else if self.s.cur.is_nested_start() {
                let p = self.in_braces_or_indented(|p| {
                    let stats = p.top_stat_seq(false);
                    p.make_packaging(start, pkg, &stats)
                });
                ts.push(p);
                true
            } else {
                self.accept_stat_sep();
                let stats = self.top_stats();
                ts.push(self.make_packaging(start, pkg, &stats));
                false
            }
        };
        if more {
            self.stat_sep_or_end(&mut ts, false, "toplevel definition", Token::Eof);
            ts.extend(self.top_stat_seq(false));
        }
        ts
    }

    fn make_packaging(&mut self, start: Offset, pkg: NodeId, stats: &[NodeId]) -> NodeId {
        let point = self.point_offset(pkg);
        let stats = self.ast.list(stats);
        let t = self.ast.node(Tree::PackageDef { pid: pkg, stats });
        self.at_span2(start, point, t)
    }

    /// `package QualId [nl] { TopStatSeq }`.
    fn packaging(&mut self, start: Offset) -> NodeId {
        self.nested(|p| p.packaging_at_level(start))
    }

    fn packaging_at_level(&mut self, start: Offset) -> NodeId {
        let pkg = self.qual_id();
        self.new_template_start();
        if !matches!(self.tok(), Token::Indent | Token::LBrace) {
            let suffix = "\nNested package statements that are not at the beginning of the file require braces or ':' with an indented body.";
            let msg = Self::expected_but_found(Token::LBrace, self.tok(), "':' or ", suffix);
            let l = self.last_offset();
            self.syntax_error_or_incomplete(msg, l);
        }
        let stats = self.in_braces_or_indented(|p| p.top_stat_seq(false));
        self.make_packaging(start, pkg, &stats)
    }

    /// `TopStat {semi TopStat}`.
    fn top_stat_seq(&mut self, outermost: bool) -> Vec<NodeId> {
        let mut stats = Vec::new();
        loop {
            let mut empty = false;
            if self.tok() == Token::Package {
                let start = self.s.skip_token();
                if self.tok() == Token::Object {
                    self.next();
                    stats.push(self.object_def(start, Modifiers::with_flags(flags::PACKAGE)));
                } else {
                    stats.push(self.packaging(start));
                }
            } else if self.tok() == Token::Import {
                stats.extend(self.import_clause(outermost));
            } else if self.tok() == Token::Export {
                stats.extend(self.export_clause());
            } else if self.is_ident_named("extension") && self.following_is_extension() {
                stats.push(self.extension());
            } else if self.is_def_intro(MODIFIER_TOKENS, false) {
                let start = self.offset();
                let mods = self.def_annots_mods(MODIFIER_TOKENS);
                let t = self.def_or_dcl(start, mods);
                if !matches!(self.ast.tree(t), Tree::Empty) {
                    stats.push(t);
                }
            } else {
                empty = true;
            }
            if !self.stat_sep_or_end(&mut stats, empty, "toplevel definition", Token::Eof) {
                return stats;
            }
        }
    }
}
