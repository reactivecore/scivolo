// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Matches the desugaring makes: pattern-matching lambdas and checked selectors.

use super::Desugar;
use crate::ast::flags;
use crate::ast::tree::MatchKind;
use crate::ast::{Attrs, NodeId, Tree};

/// How the selector of a desugared match is checked (dotty's `MatchCheck`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MatchCheck {
    /// `sel: @unchecked`.
    Unchecked,
    PatDef,
    GenFrom,
}

impl Desugar<'_> {
    /// `sel` checked as `check` says (dotty's `makeSelector`).
    pub fn make_selector(&mut self, sel: NodeId, check: MatchCheck) -> NodeId {
        let (set, other) = match check {
            MatchCheck::Unchecked => {
                let annot = self.new_annot("scala.unchecked");
                return self.ast.node(Tree::Annotated { arg: sel, annot });
            }
            MatchCheck::PatDef => (Attrs::CHECK_PAT_DEF, Attrs::CHECK_GEN_FROM),
            MatchCheck::GenFrom => (Attrs::CHECK_GEN_FROM, Attrs::CHECK_PAT_DEF),
        };
        self.ast.remove_attrs(sel, other);
        self.ast.add_attrs(sel, set);
        sel
    }

    /// `x$1 => x$1 match { cases }` (dotty's `makeCaseLambda` of one parameter).
    pub fn make_case_lambda(&mut self, cases: &[NodeId], check: MatchCheck) -> NodeId {
        let name = self.term("x$1");
        let tpt = self.type_tree();
        let mods = self.mods(flags::SYNTHETIC_TERM_PARAM);
        let param = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
        let selector = self.ident(name);
        let selector = self.make_selector(selector, check);
        let body = self.make_match(selector, cases);
        let args = self.list(&[param]);
        self.ast.node(Tree::Function { args, body, mods: None, wildcard: false })
    }

    /// `rhs match { case pat => body }` with the selector checked as for a pattern definition.
    pub fn single_case_match(&mut self, rhs: NodeId, pat: NodeId, body: NodeId) -> NodeId {
        let case = self.ast.node(Tree::CaseDef { pat, guard: None, body });
        let selector = self.make_selector(rhs, MatchCheck::PatDef);
        self.make_match(selector, &[case])
    }

    fn make_match(&mut self, selector: NodeId, cases: &[NodeId]) -> NodeId {
        let cases = self.list(cases);
        self.ast.node(Tree::Match { selector: Some(selector), cases, kind: MatchKind::Plain })
    }
}
