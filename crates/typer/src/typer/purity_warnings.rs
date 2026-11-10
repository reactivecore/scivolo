// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The warnings about pure expressions in statement position or discarded for `Unit` (dotty's
//! `checkStatementPurity`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;

impl Typer<'_> {
    /// Warns at `at` about pure statement `tree`, unless it is an inline call expanding to `()`.
    pub fn check_statement_purity(&mut self, tree: TNodeId, at: syntax::ast::Span) {
        let callee = self.tpd.meth_part(tree);
        let constr_call = self.tpd.sym(callee).is_some_and(|s| self.u.symbol(s).name == "<init>")
            && matches!(*self.tpd.tree(callee), T::Select { qualifier, .. }
                if matches!(self.tpd.tree(qualifier), T::This { .. } | T::Super { .. }));
        let unit_inline = matches!(*self.tpd.tree(tree), T::Inlined { bindings, expansion, .. }
            if self.tpd.items(bindings).is_empty()
                && matches!(self.tpd.tree(expansion), T::Literal { value: Constant::Unit }));
        if self.is_pure_expr(tree) && !constr_call && !unit_inline {
            let msg = "A pure expression does nothing in statement position";
            self.diags.warning_at(at.start(), at.end(), at.point(), msg);
        }
    }

    /// Warns about pure expression `tree` whose value is discarded for an expected `Unit`.
    pub fn check_unit_purity(&mut self, tree: TNodeId) {
        if self.is_pure_expr(tree) {
            let tpe = self.tpd.tpe(tree);
            let widened = self.u.widen(tpe);
            let shown = self.plain_text(widened).unwrap_or_else(|| self.types().show_type(widened));
            let msg = format!("Discarded non-Unit value of type {shown}. Add `: Unit` to discard silently.");
            let at = self.tpd.span(tree).point();
            self.diags.warning(at, msg);
        }
    }
}
