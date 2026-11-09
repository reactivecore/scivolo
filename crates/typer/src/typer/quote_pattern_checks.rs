// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/QuotePatterns.scala

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use syntax::diag::Level;
use tasty::SymbolId;

impl Typer<'_> {
    /// Reports the trees in quote pattern body `body` that a quote pattern cannot match and the
    /// higher-order splice arguments of type variables they do not capture (dotty's
    /// `QuotePatterns.checkPattern`).
    pub fn check_quote_pattern(&mut self, body: TNodeId) {
        let (mut type_vars, mut splices) = (Vec::new(), Vec::new());
        self.check_quote_pattern_tree(body, &mut type_vars, &mut splices);
        for s in splices {
            let T::SplicePattern { args, .. } = *self.tpd.tree(s) else { continue };
            for a in self.tpd.items(args).to_vec() {
                let tpe = self.u.widen(self.tpd.tpe(a));
                if self.refers_to(tpe, &type_vars) {
                    let msg =
                        "Type variables that this argument depends on are not captured in this hoas pattern";
                    self.error_at(self.tpd.span(a), msg);
                }
            }
        }
    }

    /// Checks `t` and appends the type parameters of its methods to `type_vars`, its splice
    /// patterns to `splices`.
    fn check_quote_pattern_tree(
        &mut self,
        t: TNodeId,
        type_vars: &mut Vec<SymbolId>,
        splices: &mut Vec<TNodeId>,
    ) {
        let class = self.tpd.sym(t).filter(|&s| self.u.is_class(s));
        let what = match *self.tpd.tree(t) {
            T::SplicePattern { body, .. } => {
                if !self.derives_from_named(self.tpd.tpe(body), "scala.quoted.Expr") {
                    self.error_at(self.tpd.span(body), "Splice pattern must match an Expr[...]");
                }
                splices.push(t);
                return;
            }
            T::Match { .. } => "`match` expressions",
            T::Try { .. } => "`try` expressions",
            T::Return { .. } => "`return` statements",
            T::TypeDef { .. } if class.is_some_and(|c| self.u.symbol(c).flags.has(tasty::tags::OBJECT)) => {
                "objects"
            }
            T::TypeDef { .. } if class.is_some() => "classes",
            T::ValDef { name, .. }
            | T::DefDef { name, .. }
            | T::TypeDef { name, .. }
            | T::Bind { name, .. } => {
                self.check_quoted_def_name(t, name, type_vars);
                ""
            }
            _ => "",
        };
        if what.is_empty() {
            for c in self.tpd.children(t) {
                self.check_quote_pattern_tree(c, type_vars, splices);
            }
        } else {
            self.error_at(self.tpd.span(t), format!("Implementation restriction: cannot match {what}"));
        }
    }

    /// Checks the name of definition `t` in a quote pattern; a method's type parameters go to
    /// `type_vars`.
    fn check_quoted_def_name(&mut self, t: TNodeId, name: TName, type_vars: &mut Vec<SymbolId>) {
        let span = self.tpd.span(t);
        if let T::DefDef { paramss, .. } = *self.tpd.tree(t) {
            for &clause in self.tpd.clause_items(paramss) {
                for &p in self.tpd.items(clause) {
                    if let (T::TypeDef { .. }, Some(s)) = (self.tpd.tree(p), self.tpd.sym(p)) {
                        type_vars.push(s);
                    }
                }
            }
        }
        let text = self.text(name.name);
        if text.starts_with("_$") {
            let msg = "Use of `_` for lambda in quoted pattern. Use explicit lambda instead or use `$_` to match any term.";
            self.warning_at(span, msg);
        } else if !name.is_type && !span.is_synthetic() && text != "$anonfun" && text.starts_with('$') {
            let at = span.point();
            let name_span = Span::new(at, at + text.len() as u32, at);
            self.diags.items.retain(|d| d.level != Level::Warning || d.offset != Some(at));
            self.error_at(name_span, "Names cannot start with $ quote pattern");
        }
    }
}
