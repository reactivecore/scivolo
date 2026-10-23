// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameOps.scala
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Alphanumeric infix operations under `-source future` (dotty's `checkValidInfix`).

use super::Typer;
use syntax::ast::{Attrs, NodeId, Tree};
use syntax::scan::chars::{is_identifier_part, is_operator_part};
use tasty::SymbolId;
use tasty::tags::{CASE, INFIX};

impl Typer<'_> {
    /// Whether call `id` of `meth` through `fun` is an infix operation with an alphanumeric
    /// operator, not backquoted, that `meth` does not declare `infix`, under `-source future`.
    pub fn is_invalid_future_infix(&mut self, id: NodeId, fun: NodeId, meth: Option<SymbolId>) -> bool {
        if !self.source_future || !self.ast.attrs(id).contains(Attrs::WAS_TYPED_INFIX) {
            return false;
        }
        let (&Tree::Select { name, .. }, Tree::Apply { args, .. }) = (self.ast.tree(fun), self.ast.tree(id))
        else {
            return false;
        };
        let text = self.text(name.name).to_owned();
        let span = self.ast.span(fun);
        let backquoted = (span.end() - span.point()) as usize != text.len();
        let block_arg = matches!(self.ast.items(*args), [a] if matches!(self.ast.tree(*a), Tree::Block { .. } | Tree::Match { .. }));
        !backquoted && !block_arg && self.is_undeclared_alphanumeric(&text, meth)
    }

    /// Whether infix type or pattern `id` with operator `op` of `sym` is invalid as in
    /// `is_invalid_future_infix`.
    pub fn is_invalid_future_infix_op(&mut self, id: NodeId, op: NodeId, sym: Option<SymbolId>) -> bool {
        if !self.source_future || !self.ast.attrs(id).contains(Attrs::INFIX_OP) {
            return false;
        }
        let &Tree::Ident { name } = self.ast.tree(op) else { return false };
        let text = self.text(name.name).to_owned();
        !self.ast.attrs(op).contains(Attrs::BACKQUOTED) && self.is_undeclared_alphanumeric(&text, sym)
    }

    fn is_undeclared_alphanumeric(&mut self, name: &str, sym: Option<SymbolId>) -> bool {
        let declared = sym.is_some_and(|m| {
            let owner = self.u.symbol(m).owner;
            self.is_declared_infix(m) || owner.is_some_and(|o| self.u.is_scala2(o))
        });
        !is_operator_name(name) && !declared
    }

    /// dotty's `isDeclaredInfix`: `infix`, or the extractor of an `infix` case class.
    fn is_declared_infix(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        if sym.flags.has(INFIX) {
            return true;
        }
        let (Some(owner), true) = (sym.owner, matches!(sym.name.as_str(), "unapply" | "unapplySeq")) else {
            return false;
        };
        let class = self.u.is_module_class(owner).then(|| self.u.companion_class(owner)).flatten();
        class.is_some_and(|c| self.u.symbol(c).flags.has(CASE) && self.is_declared_infix(c))
    }
}

/// dotty's `isOperatorName`: operator characters, optionally after an identifier and `_`.
fn is_operator_name(name: &str) -> bool {
    let alnum = name.trim_end_matches(is_operator_part);
    alnum.is_empty() || alnum.ends_with('_') && alnum.chars().all(is_identifier_part)
}
