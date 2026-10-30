// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Cases and patterns of `Printer.TreeCode`.

use super::{Code, REFUSED, Shown};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    pub fn cases(&mut self, cases: &[TNodeId]) -> Shown {
        self.push(" {");
        self.indent += 1;
        self.line_break();
        for (i, &c) in cases.iter().enumerate() {
            if i > 0 {
                self.line_break();
            }
            self.case_def(c)?;
        }
        self.indent -= 1;
        self.line_break();
        self.push("}");
        Ok(())
    }

    pub fn case_def(&mut self, c: TNodeId) -> Shown {
        let T::CaseDef { pat, guard, body } = self.tree_of(c) else { return Err(REFUSED) };
        self.definition_name("case ");
        self.pattern(pat)?;
        if let Some(g) = guard {
            self.push(" if ");
            self.tree(g)?;
        }
        self.definition_name(" =>");
        self.indent += 1;
        match self.tree_of(body) {
            T::Block { stats, expr } => self.stats(&self.items(stats), expr)?,
            _ => {
                self.line_break();
                self.tree(body)?;
            }
        }
        self.indent -= 1;
        Ok(())
    }

    fn is_wildcard(&self, t: TNodeId) -> bool {
        matches!(self.tree_of(t), T::Ident { name } if self.name(name) == "_")
    }

    pub fn pattern(&mut self, t: TNodeId) -> Shown {
        let t = self.r(t);
        match self.tree_of(t) {
            T::Bind { name, body } => {
                let name = self.name(name);
                match self.tree_of(body) {
                    _ if self.is_wildcard(body) => self.push(&name),
                    T::Typed { expr, tpt } if self.is_wildcard(expr) => {
                        self.definition_name(&name);
                        self.push(": ");
                        self.type_tree(tpt)?;
                    }
                    _ => {
                        self.push(&format!("{name} @ "));
                        self.pattern(body)?;
                    }
                }
            }
            T::UnApply { fun, patterns, .. } => {
                let fun = match self.tree_of(fun) {
                    T::TypeApply { fun, .. } => fun,
                    _ => fun,
                };
                let T::Select { qualifier, name } = self.tree_of(fun) else { return Err(REFUSED) };
                if !matches!(&*self.name(name), "unapply" | "unapplySeq") {
                    return Err(REFUSED);
                }
                self.tree(qualifier)?;
                self.push("(");
                self.patterns(&self.items(patterns), ", ")?;
                self.push(")");
            }
            T::Alternative { trees } => {
                self.push("(");
                self.patterns(&self.items(trees), " | ")?;
                self.push(")");
            }
            T::Typed { expr, tpt } if self.is_wildcard(expr) => {
                self.push("_: ");
                self.type_tree(tpt)?;
            }
            T::Typed { expr, .. } => self.pattern(expr)?,
            _ => self.tree(t)?,
        }
        Ok(())
    }

    fn patterns(&mut self, pats: &[TNodeId], sep: &str) -> Shown {
        for (i, &p) in pats.iter().enumerate() {
            if i > 0 {
                self.push(sep);
            }
            self.pattern(p)?;
        }
        Ok(())
    }
}
