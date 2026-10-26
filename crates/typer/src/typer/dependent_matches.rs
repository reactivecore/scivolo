// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Matches typed against a match type of the same shape (dotty's `isMatchTypeShaped`,
//! `typedDependentMatchFinish`, `instantiateMatchTypeProto`).

use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{ANY, Type, TypeId};

impl Typer<'_> {
    /// The cases of match type `pt` if a match on `sel` with cases `cases` has its shape: as many
    /// cases, no guards, typed or wildcard patterns, the selector within the scrutinee.
    pub fn dependent_match_cases(&mut self, pt: Pt, sel: TNodeId, cases: &[NodeId]) -> Option<Vec<TypeId>> {
        let Pt::Type(pt) = pt else { return None };
        let m = self.u.underlying_match(pt)?;
        let Type::Match { scrutinee, cases: case_types, .. } = self.u.types().get(m).clone() else {
            return None;
        };
        if case_types.len() != cases.len() || !cases.iter().all(|&c| self.is_dependent_case_shape(c)) {
            return None;
        }
        let sel_tpe = self.tpd.tpe(sel);
        self.u.is_subtype(sel_tpe, scrutinee).then_some(case_types)
    }

    /// Whether case `c` has no guard and a pattern `x: T`, `x @ (_: T)` or `_`.
    fn is_dependent_case_shape(&self, c: NodeId) -> bool {
        let Tree::CaseDef { pat, guard: None, .. } = *self.ast.tree(c) else { return false };
        let pat = match *self.ast.tree(pat) {
            Tree::Bind { body, .. } => body,
            _ => pat,
        };
        match self.ast.tree(pat) {
            Tree::Typed { .. } => true,
            Tree::Ident { name } => self.text(name.name) == "_",
            _ => false,
        }
    }

    /// The expected type of the body of a case with typed pattern `pat` against match type case
    /// `case`, its type variables those `pat` binds; `None` if the patterns differ.
    pub fn dependent_case_body_pt(&mut self, pat: TNodeId, case: TypeId) -> Option<TypeId> {
        let pat = match *self.tpd.tree(pat) {
            T::Bind { body, .. } => body,
            _ => pat,
        };
        let pat_tpe = match *self.tpd.tree(pat) {
            T::Typed { tpt, .. } => self.tpd.tpe(tpt),
            T::Ident { .. } => ANY,
            _ => return None,
        };
        let case = match self.u.types().get(case).clone() {
            Type::Lambda { binder, params, result } => {
                let mut bound = Vec::new();
                self.pattern_types(pat, &mut bound, false);
                if bound.len() != params.len() {
                    return None;
                }
                let args: Vec<TypeId> =
                    bound.iter().map(|&(s, _)| self.u.intern_type(Type::Sym(None, s))).collect();
                let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                self.u.subst(result, binder, &names, &args)
            }
            _ => case,
        };
        let Type::MatchCase(case_pat, body) = *self.u.types().get(case) else { return None };
        let same = self.u.is_subtype(pat_tpe, case_pat) && self.u.is_subtype(case_pat, pat_tpe);
        same.then_some(body)
    }

    /// Case body `body` of a dependent match, typed against `pt`, ascribed `pt` if its type
    /// refers to the pattern's variables.
    pub fn dependent_case_body(&mut self, body: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(body);
        let tpe = self.fully_defined_as(tpe, true).ok()?;        self.u.is_subtype(tpe, pt).then(|| self.ascribed(body, pt))
    }
}
