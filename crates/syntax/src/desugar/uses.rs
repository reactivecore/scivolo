// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/ast/untpd.scala
//! The `uses` clause of a class as `@retains` annotations (dotty's `addImplUse` and
//! `makeRetainsAnnot`).

use super::Desugar;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, Tree};

impl Desugar<'_> {
    /// `m` with an annotation `@retains[r1 | ... | rN]` for the references of `uses` that are
    /// (or are not) `initially`.
    pub fn add_impl_use(&mut self, m: Modifiers, uses: &[NodeId], initially: bool) -> Modifiers {
        let refs: Vec<NodeId> = uses
            .iter()
            .filter_map(|&u| match *self.ast.tree(u) {
                Tree::UseRef { reference, initially: i } if i == initially => Some(reference),
                _ => None,
            })
            .collect();
        if refs.is_empty() {
            return m;
        }
        let singletons: Vec<NodeId> =
            refs.into_iter().map(|reference| self.ast.node(Tree::SingletonTypeTree { reference })).collect();
        let trefs = singletons
            .into_iter()
            .reduce(|a, b| {
                let or = self.sym_ref("scala.|", true);
                let args = self.list(&[a, b]);
                self.ast.node(Tree::AppliedTypeTree { tpt: or, args })
            })
            .expect("a reference");
        let (annotation, retains) = (self.term("annotation"), self.type_name("retains"));
        let package = self.scala_dot(annotation);
        let constr = self.select(package, retains);
        let args = self.list(&[trefs]);
        let tpt = self.ast.node(Tree::AppliedTypeTree { tpt: constr, args });
        let nu = self.make_new(tpt);
        let annot = self.apply(nu, &[]);
        self.ast.add_attrs(annot, Attrs::RETAINS_ANNOT);
        let annotations = [self.ast.items(m.annotations), &[annot]].concat();
        let annotations = self.list(&annotations);
        Modifiers { annotations, ..m }
    }
}
