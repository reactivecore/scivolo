// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! References of `Printer.TreeCode`: identifiers, selections, `this` and `super`.

use super::{Code, REFUSED, Shown};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    /// Reference `t`.
    pub fn reference(&mut self, t: TNodeId) -> Shown {
        match self.tree_of(t) {
            T::Ident { name } if self.name(name) == "_" => self.push("_"),
            T::Ident { .. } => match self.h.tpd.sym(t).filter(|&s| self.h.u.is_local(s)) {
                Some(s) => {
                    let name = self.spliced_name(s);
                    self.highlight_type(&name);
                }
                None => self.ty(self.h.tpd.tpe(t))?,
            },
            T::Select { qualifier, name } => {
                self.qual_tree(qualifier)?;
                let name = self.name(name);
                if name != "<init>" && name != "package" {
                    self.push(".");
                    self.push(&name);
                }
            }
            T::This { qual } => {
                if let T::Ident { name } = self.tree_of(qual) {
                    let name = self.name(name);
                    if !name.is_empty() {
                        self.push(name.strip_suffix('$').unwrap_or(&name));
                        self.push(".");
                    }
                }
                self.push("this");
            }
            T::Super { qual, mix } => {
                if let T::This { qual } = self.tree_of(qual)
                    && let T::Ident { name } = self.tree_of(qual)
                    && !self.name(name).is_empty()
                {
                    self.push(&format!("{}.", self.name(name)));
                }
                self.push("super");
                if let T::Ident { name } = self.tree_of(mix)
                    && !self.name(name).is_empty()
                {
                    self.push(&format!("[{}]", self.name(name)));
                }
            }
            _ => return Err(REFUSED),
        }
        Ok(())
    }
}
