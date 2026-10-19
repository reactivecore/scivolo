// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/untpd.scala
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Trees for `new` and for references to library classes.

use super::Parser;
use crate::ast::{NodeId, TName, Tree};

impl Parser<'_> {
    /// `Select(New(tpt), <init>)` (dotty's `wrapNew`).
    pub fn wrap_new(&mut self, tpt: NodeId) -> NodeId {
        let new = self.ast.node(Tree::New { tpt });
        let init = self.term("<init>");
        self.ast.node(Tree::Select { qualifier: new, name: init })
    }

    /// `new tpt(args)` for one argument list, with `tpt`'s type arguments on the constructor
    /// (dotty's `New(tpt, args :: Nil)`).
    pub fn make_new(&mut self, tpt: NodeId, args: &[NodeId]) -> NodeId {
        let (tycon, targs) = match *self.ast.tree(tpt) {
            Tree::AppliedTypeTree { tpt, args } => (tpt, Some(args)),
            _ => (tpt, None),
        };
        let new = self.ast.node(Tree::New { tpt: tycon });
        let init = self.term("<init>");
        let mut fun = self.ast.node(Tree::Select { qualifier: new, name: init });
        if let Some(targs) = targs {
            fun = self.ast.node(Tree::TypeApply { fun, args: targs });
        }
        let args = self.ast.list(args);
        self.ast.node(Tree::Apply { fun, args })
    }

    /// `_root_.scala.<terms>.<last>`.
    pub fn scala_path(&mut self, terms: &[&str], last: TName) -> NodeId {
        let (first, rest) = terms.split_first().expect("a package");
        let first = self.term(first);
        let mut t = self.scala_dot(first);
        for name in rest {
            let name = self.term(name);
            t = self.ast.node(Tree::Select { qualifier: t, name });
        }
        self.ast.node(Tree::Select { qualifier: t, name: last })
    }
}
