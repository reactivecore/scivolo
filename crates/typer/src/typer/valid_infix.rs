// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameOps.scala
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Alphanumeric infix operations (dotty's `checkValidInfix`).

use super::Typer;
use syntax::ast::{Attrs, NodeId, Span, Tree};
use syntax::scan::chars::{is_identifier_part, is_operator_part};
use tasty::SymbolId;
use tasty::tags::{CASE, INFIX};

impl Typer<'_> {
    /// Checks call `id` of `meth` through `fun` written infix: an alphanumeric operator, not
    /// backquoted, that `meth` does not declare `infix` is a warning, and under `-source future`
    /// an error; whether it is one.
    pub fn is_invalid_infix(&mut self, id: NodeId, fun: NodeId, meth: Option<SymbolId>) -> bool {
        if !self.ast.attrs(id).contains(Attrs::WAS_TYPED_INFIX) {
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
        if backquoted || block_arg || !self.is_undeclared_alphanumeric(&text, meth) {
            return false;
        }
        let op = Span::new(span.point(), span.end(), span.point());
        self.invalid_infix(op, "method", format!("method syntax .{text}(...)"), &text)
    }

    /// Checks infix type or pattern `id` with operator `op` of `sym` as
    /// `is_invalid_infix`.
    pub fn is_invalid_infix_op(&mut self, id: NodeId, op: NodeId, sym: Option<SymbolId>) -> bool {
        if !self.ast.attrs(id).contains(Attrs::INFIX_OP) {
            return false;
        }
        let &Tree::Ident { name } = self.ast.tree(op) else { return false };
        let text = self.text(name.name).to_owned();
        if self.ast.attrs(op).contains(Attrs::BACKQUOTED) || !self.is_undeclared_alphanumeric(&text, sym) {
            return false;
        }
        let (kind, alternative) = match name.is_type {
            true => ("type", format!("prefix syntax {text}[...]")),
            false => ("extractor", format!("prefix syntax {text}(...)")),
        };
        self.invalid_infix(self.ast.span(op), kind, alternative, &text)
    }

    /// dotty's `errorOrMigrationWarning` with `MigrationVersion.AlphanumericInfix`.
    fn invalid_infix(&mut self, op: Span, kind: &str, alternative: String, name: &str) -> bool {
        if self.source_future || self.scala2 {
            return self.source_future;
        }
        let msg = format!(
            "Alphanumeric {kind} {name} is not declared infix; it should not be used as infix operator.\n\
             Instead, use {alternative} or backticked identifier `{name}`.\n\
             The latter can be rewritten automatically under -rewrite -source 3.4-migration."
        );
        self.warning_at(op, msg);
        false
    }

    fn is_undeclared_alphanumeric(&mut self, name: &str, sym: Option<SymbolId>) -> bool {
        let declared = sym.is_some_and(|m| {
            let owner = self.u.symbol(m).owner;
            self.is_declared_infix(m)
                || owner.is_some_and(|o| self.u.is_scala2(o))
                || self.u.compiled_before_3_4(m)
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
