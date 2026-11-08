// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Matches, `try`, `return` and patterns (dotty's `Match`, `CaseDef`, `Try`, `Return`, `Bind`,
//! `Alternative` and `UnApply` cases of `pickleTree`).

use super::names::local_name;
use super::{Refusal, TreePickler};
use crate::tpd::{List, MatchKind, TNodeId};
use tasty::tags::*;

impl TreePickler<'_> {
    pub fn match_(&mut self, selector: Option<TNodeId>, cases: List, kind: MatchKind) -> Result<(), Refusal> {
        let cases = self.tpd.items(cases).to_vec();
        self.with_length(MATCH, &mut |p| {
            match (kind, selector) {
                (MatchKind::Inline, None) => p.byte(IMPLICIT),
                (MatchKind::Inline, Some(_)) => p.byte(INLINE),
                (MatchKind::Sub, _) => p.byte(SUBMATCH),
                (MatchKind::Plain, _) => {}
            }
            selector.into_iter().chain(cases.iter().copied()).try_for_each(|t| p.tree(t))
        })
    }

    pub fn case_def(&mut self, pat: TNodeId, guard: Option<TNodeId>, body: TNodeId) -> Result<(), Refusal> {
        self.with_length(CASEDEF, &mut |p| {
            [Some(pat), Some(body), guard].into_iter().flatten().try_for_each(|t| p.tree(t))
        })
    }

    pub fn try_(&mut self, expr: TNodeId, cases: List, finalizer: Option<TNodeId>) -> Result<(), Refusal> {
        let all: Vec<TNodeId> =
            [expr].into_iter().chain(self.tpd.items(cases).iter().copied()).chain(finalizer).collect();
        self.with_length(TRY, &mut |p| all.iter().try_for_each(|&t| p.tree(t)))
    }

    /// `return expr` from the method `from` names.
    pub fn return_(&mut self, expr: TNodeId, from: TNodeId) -> Result<(), Refusal> {
        let method = self.tpd.sym(from).ok_or("a return from no method")?;
        self.with_length(RETURN, &mut |p| {
            p.sym_addr(method);
            p.tree(expr)
        })
    }

    /// A pattern variable `id`: its name, type, pattern and modifiers but `case`.
    pub fn bind(&mut self, id: TNodeId, body: TNodeId) -> Result<(), Refusal> {
        let sym = self.tpd.sym(id).ok_or("a pattern variable without a symbol")?;
        self.register_def(sym);
        let info = self.u.info(sym);
        let (flags, term) = (self.u.symbol(sym).flags.without(CASE), self.u.symbol(sym).kind.is_term());
        self.with_length(BIND, &mut |p| {
            p.name(&local_name(p.u, sym));
            p.ty(info, false)?;
            p.tree(body)?;
            p.flags(flags, term);
            Ok(())
        })
    }

    /// An extractor pattern: its extractor, implicit arguments, type and patterns.
    pub fn unapply(
        &mut self,
        id: TNodeId,
        fun: TNodeId,
        implicits: List,
        patterns: List,
    ) -> Result<(), Refusal> {
        let tpe = self.tpd.get(id).tpe.ok_or("an extractor pattern without a type")?;
        let (implicits, patterns) = (self.tpd.items(implicits).to_vec(), self.tpd.items(patterns).to_vec());
        self.with_length(UNAPPLY, &mut |p| {
            p.tree(fun)?;
            for &i in &implicits {
                p.byte(IMPLICITarg);
                p.tree(i)?;
            }
            p.ty(tpe, false)?;
            patterns.iter().try_for_each(|&t| p.tree(t))
        })
    }
}
