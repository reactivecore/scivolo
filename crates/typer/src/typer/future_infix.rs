// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Alphanumeric infix calls under `-source future` (dotty's `checkValidInfix`).

use super::Typer;
use syntax::ast::{Attrs, NodeId, Tree};
use syntax::scan::chars::is_operator_part;
use tasty::SymbolId;

impl Typer<'_> {
    /// Whether call `id` of `meth` through `fun` is an infix operation with an alphanumeric
    /// operator, not backquoted, that `meth` does not declare `infix`, under `-source future`.
    pub fn is_invalid_future_infix(&self, id: NodeId, fun: NodeId, meth: Option<SymbolId>) -> bool {
        if !self.source_future || !self.ast.attrs(id).contains(Attrs::WAS_TYPED_INFIX) {
            return false;
        }
        let (Tree::Select { name, .. }, Tree::Apply { args, .. }) = (self.ast.tree(fun), self.ast.tree(id))
        else {
            return false;
        };
        let text = self.text(name.name);
        let span = self.ast.span(fun);
        let backquoted = (span.end() - span.point()) as usize != text.len();
        let alphanumeric = text.chars().last().is_some_and(|c| !is_operator_part(c));
        let block_arg = matches!(self.ast.items(*args), [a] if matches!(self.ast.tree(*a), Tree::Block { .. } | Tree::Match { .. }));
        let declared = meth.is_some_and(|m| {
            let owner = self.u.symbol(m).owner;
            self.u.symbol(m).flags.has(tasty::tags::INFIX) || owner.is_some_and(|o| self.u.is_scala2(o))
        });
        !backquoted && alphanumeric && !block_arg && !declared
    }
}
