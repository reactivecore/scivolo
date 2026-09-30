//! The trees capture checking adds: `@retains` annotations, capability markers, `CapSet`.

use crate::ast::{Attrs, NodeId, Tree};
use crate::parse::Parser;

impl Parser<'_> {
    /// `new tpt()`.
    fn new_annot(&mut self, tpt: NodeId) -> NodeId {
        self.make_new(tpt, &[])
    }

    /// `new scala.annotation.internal.<name>()`.
    pub fn internal_annot(&mut self, name: &str) -> NodeId {
        let name = self.type_name(name);
        let tpt = self.scala_path(&["annotation", "internal"], name);
        self.new_annot(tpt)
    }

    /// `new scala.annotation.internal.onlyCapability[qid]()`.
    pub fn only_annot(&mut self, qid: NodeId) -> NodeId {
        let name = self.type_name("onlyCapability");
        let tycon = self.scala_path(&["annotation", "internal"], name);
        let args = self.ast.list(&[qid]);
        let tpt = self.ast.node(Tree::AppliedTypeTree { tpt: tycon, args });
        self.new_annot(tpt)
    }

    /// `new scala.caps.internal.consume()`.
    pub fn consume_annot(&mut self) -> NodeId {
        let name = self.type_name("consume");
        let tpt = self.scala_path(&["caps", "internal"], name);
        self.new_annot(tpt)
    }

    /// `parent @retains[refs]`, or `parent @retainsCap` without `refs` (dotty's `makeRetaining`).
    pub fn make_retaining(&mut self, parent: NodeId, refs: Option<&[NodeId]>) -> NodeId {
        let annot = match refs {
            None => {
                let name = self.type_name("retainsCap");
                let tpt = self.scala_path(&["annotation"], name);
                self.new_annot(tpt)
            }
            Some(refs) => {
                let trefs = match refs {
                    [] => {
                        let sym = self.type_name("scala.Nothing");
                        self.ast.node(Tree::TypedSplice { sym })
                    }
                    _ => {
                        let singletons: Vec<NodeId> = refs
                            .iter()
                            .map(|&reference| self.ast.node(Tree::SingletonTypeTree { reference }))
                            .collect();
                        singletons.into_iter().reduce(|a, b| self.make_or_type(a, b)).expect("a reference")
                    }
                };
                let name = self.type_name("retains");
                let tycon = self.scala_path(&["annotation"], name);
                let args = self.ast.list(&[trefs]);
                let tpt = self.ast.node(Tree::AppliedTypeTree { tpt: tycon, args });
                let annot = self.new_annot(tpt);
                self.ast.add_attrs(annot, Attrs::RETAINS_ANNOT);
                annot
            }
        };
        self.ast.node(Tree::Annotated { arg: parent, annot })
    }

    /// `A | B` (dotty's `makeOrType`).
    fn make_or_type(&mut self, left: NodeId, right: NodeId) -> NodeId {
        let sym = self.type_name("scala.|");
        let or = self.ast.node(Tree::TypedSplice { sym });
        let args = self.ast.list(&[left, right]);
        self.ast.node(Tree::AppliedTypeTree { tpt: or, args })
    }

    /// `{caps.any} tp`, the result of an impure by-name type `=> tp` (dotty's
    /// `ImpureByNameTypeTree`).
    pub fn impure_result(&mut self, tp: NodeId) -> NodeId {
        let any = self.term("any");
        let root = self.scala_path(&["caps"], any);
        let at = self.ast.span(tp).start_pos();
        self.ast.with_span(root, at);
        let refs = self.ast.list(&[root]);
        self.ast.node(Tree::CapturesAndResult { refs, parent: tp })
    }

    /// `scala.caps.CapSet`.
    fn cap_set(&mut self) -> NodeId {
        let name = self.type_name("CapSet");
        self.scala_path(&["caps"], name)
    }

    /// `CapSet @retains[refs]` (dotty's `concreteCapsType`).
    fn concrete_caps_type(&mut self, refs: &[NodeId]) -> NodeId {
        let cap_set = self.cap_set();
        self.make_retaining(cap_set, Some(refs))
    }

    /// A capture set as a type where capture checking allows one: `CapSet @retains[refs]`, or
    /// `CapSet` alone for an empty lower bound.
    pub fn capture_set_type(&mut self, lower_bound: bool) -> Option<NodeId> {
        if !self.at_capture_set() {
            return None;
        }
        let refs = self.capture_set();
        Some(if lower_bound && refs.is_empty() { self.cap_set() } else { self.concrete_caps_type(&refs) })
    }
}
