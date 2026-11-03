// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
use crate::symbols::Completion;
use crate::typer::Typer;
use syntax::ast::Tree;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Reports implicit method `sym`, or implicit value of a class, whose type is inferred, and
    /// makes it a plain one (dotty's `missingType`).
    pub fn check_inferred_implicit(&mut self, sym: SymbolId) {
        if !self.u.symbol(sym).flags.has(tasty::tags::IMPLICIT) {
            return;
        }
        let Some(&Completion::Completing(tree)) = self.syms.completions.get(&sym) else { return };
        let member = self.u.symbol(sym).owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        let modifier = match self.ast.tree(tree) {
            Tree::DefDef { .. } => "result ",
            Tree::ValDef { .. } if member => "",
            _ => return,
        };
        let span = self.ast.span(tree);
        let msg = format!("{modifier}type of implicit definition needs to be given explicitly");
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
        let flags = self.u.symbol(sym).flags.without(tasty::tags::IMPLICIT);
        self.u.set_flags(sym, flags);
    }
}
