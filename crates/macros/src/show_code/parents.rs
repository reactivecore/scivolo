// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! The parents of class definitions in `Printer.TreeCode`.

use super::{Code, REFUSED, Shown};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    /// Whether parent `p` is a call of `java.lang.Object`'s constructor or `_root_.scala.Product`
    /// or `_root_.scala.Serializable`.
    pub fn is_elided_parent(&mut self, p: TNodeId) -> bool {
        match self.tree_of(p) {
            T::Apply { fun, .. } => {
                let T::Select { qualifier, .. } = self.tree_of(self.r(fun)) else { return false };
                let T::New { tpt } = self.tree_of(self.r(qualifier)) else { return false };
                let tpe = self.h.tpd.tpe(self.r(tpt));
                self.h.u.class_symbol(tpe).is_some_and(|c| self.h.u.full_name(c) == "java.lang.Object")
            }
            T::Select { qualifier, name } if matches!(&*self.name(name), "Product" | "Serializable") => {
                matches!(self.tree_of(qualifier), T::Select { qualifier: root, name }
                    if self.name(name) == "scala"
                        && matches!(self.tree_of(root), T::Ident { name } if self.name(name) == "_root_"))
            }
            _ => false,
        }
    }

    pub fn parent(&mut self, p: TNodeId, need_empty_parens: bool) -> Shown {
        let p = self.r(p);
        if self.is_type_tree(p) {
            return self.type_tree(p);
        }
        match self.tree_of(p) {
            T::TypeApply { fun, .. } => self.parent(fun, false),
            T::Apply { fun, args } => {
                let nested = matches!(self.tree_of(self.r(fun)), T::Apply { .. });
                self.parent(fun, nested)?;
                let args = self.items(args);
                if !args.is_empty() || need_empty_parens {
                    self.push("(");
                    self.trees(&args, ", ")?;
                    self.push(")");
                }
                Ok(())
            }
            T::Select { qualifier, .. } if matches!(self.tree_of(self.r(qualifier)), T::New { .. }) => {
                self.ty(self.h.tpd.tpe(self.r(qualifier)))
            }
            _ => Err(REFUSED),
        }
    }
}
