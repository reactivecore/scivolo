// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! The code of `scala.compiletime.testing.typeChecks` parsed and typed in the context of the
//! call (dotty's `Inlines.Intrinsics.compileForErrors`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Span, Tree};
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::Level;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, Symbol};

impl Typer<'_> {
    /// Statements `code` as a block typed in `ctx`, owned by a synthetic value of the context's
    /// owner; `None` if parsing or typing them reports an error, whose diagnostics are dropped.
    pub fn typed_code_in(&mut self, code: &str, ctx: Ctx) -> Option<TNodeId> {
        let text = format!("{{\n{code}\n}}");
        let source = SourceFile::new("tasty-reflect", &text);
        let mut p = Parser::new(&source, self.names, Config::default());
        p.ast = std::mem::take(&mut *self.ast);
        p.fresh = std::mem::take(&mut *self.fresh);
        let first = p.ast.node(Tree::Empty).index();
        let stats = p.parse_stats();
        let cfg = p.s.cfg;
        let block = match stats[..] {
            [block] if !p.s.diags.has_errors() => Some(
                Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(block),
            ),
            _ => None,
        };
        let parsed = !p.s.diags.has_errors();
        let end = p.ast.node(Tree::Empty).index();
        *self.ast = std::mem::take(&mut p.ast);
        *self.fresh = std::mem::take(&mut p.fresh);
        for i in first..end {
            self.ast.get_mut(NodeId::new(i)).span = Span::NONE;
        }
        let block = block.filter(|_| parsed)?;
        let dummy = self.u.define(Symbol {
            name: "$typingDummy".to_owned(),
            kind: Kind::Val,
            owner: None,
            flags: Flags(1 << SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(dummy, ctx.owner);
        self.u.set_info(dummy, tasty::ANY);
        let (mark, erasure_mark) = (self.diags.items.len(), self.erasure_diags.items.len());
        let typed = self.typed(block, Pt::Any, ctx.with_owner(dummy));
        let errors = self.diags.items[mark..].iter().any(|d| d.level == Level::Error);
        self.diags.items.truncate(mark);
        self.erasure_diags.items.truncate(erasure_mark);
        (!errors).then_some(typed)
    }
}
