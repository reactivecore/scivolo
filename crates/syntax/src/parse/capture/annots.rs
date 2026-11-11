// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/untpd.scala
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! The trees capture checking adds: `@retains` annotations, capability markers, `CapSet`.

use crate::ast::{NodeId, Tree};
use crate::parse::Parser;
use names::nme;

impl Parser<'_> {
    /// `new tpt()`.
    fn new_annot(&mut self, tpt: NodeId) -> NodeId {
        self.make_new(tpt, &[])
    }

    /// `new scala.annotation.internal.<name>()`.
    pub fn internal_annot(&mut self, name: &str) -> NodeId {
        let name = self.type_name(name);
        let tpt = self.scala_path(&[nme::ANNOTATION, nme::INTERNAL], name);
        self.new_annot(tpt)
    }

    /// `new scala.annotation.internal.onlyCapability[qid]()`.
    pub fn only_annot(&mut self, qid: NodeId) -> NodeId {
        let name = self.type_name(nme::ONLY_CAPABILITY);
        let tycon = self.scala_path(&[nme::ANNOTATION, nme::INTERNAL], name);
        let args = self.ast.list(&[qid]);
        let tpt = self.ast.node(Tree::AppliedTypeTree { tpt: tycon, args });
        self.new_annot(tpt)
    }

    /// `new scala.caps.internal.consume()`.
    pub fn consume_annot(&mut self) -> NodeId {
        let name = self.type_name(nme::CONSUME);
        let tpt = self.scala_path(&[nme::CAPS, nme::INTERNAL], name);
        self.new_annot(tpt)
    }

    /// `parent @retains[refs]`, or `parent @retainsCap` without `refs` (dotty's `makeRetaining`).
    pub fn make_retaining(&mut self, parent: NodeId, refs: Option<&[NodeId]>) -> NodeId {
        let annot = if refs.is_some() { nme::RETAINS } else { nme::RETAINS_CAP };
        self.ast.make_retaining(self.s.names, parent, refs.unwrap_or_default(), annot)
    }

    /// `{caps.any} tp`, the result of an impure by-name type `=> tp` (dotty's
    /// `ImpureByNameTypeTree`).
    pub fn impure_result(&mut self, tp: NodeId) -> NodeId {
        let any = self.term(nme::ANY);
        let root = self.scala_path(&[nme::CAPS], any);
        let at = self.ast.span(tp).start_pos();
        self.ast.with_span(root, at);
        let refs = self.ast.list(&[root]);
        self.ast.node(Tree::CapturesAndResult { refs, parent: tp })
    }

    /// `scala.caps.CapSet`.
    fn cap_set(&mut self) -> NodeId {
        let name = self.type_name(nme::CAP_SET);
        self.scala_path(&[nme::CAPS], name)
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
