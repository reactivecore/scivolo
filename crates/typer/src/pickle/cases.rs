// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Matches, `try`, `return` and patterns (dotty's `Match`, `CaseDef`, `Try`, `Return`, `Bind`,
//! `Alternative` and `UnApply` cases of `pickleTree`).

use super::names::local_name;
use super::{Refusal, TreePickler};
use crate::tpd::{List, MatchKind, TNodeId, Tree};
use tasty::Type;
use tasty::tags::*;

impl TreePickler<'_> {
    pub fn match_(&mut self, selector: Option<TNodeId>, cases: List, kind: MatchKind) -> Result<(), Refusal> {
        let cases = self.tpd.items(cases).to_vec();
        if selector.is_some_and(|s| self.is_untold_self_match(s, &cases)) {
            return Err("a match on `this` of a class whose self type has a capture set");
        }
        self.with_length(MATCH, &mut |p| {
            match (kind, selector) {
                (MatchKind::Inline, None) => p.byte(IMPLICIT),
                (MatchKind::Inline, Some(_)) => p.byte(INLINE),
                (MatchKind::Sub, _) => p.byte(SUBMATCH),
                (MatchKind::Plain, _) => {}
            }
            selector.into_iter().try_for_each(|t| p.tree(t))?;
            cases.iter().try_for_each(|&t| {
                p.scrutinee = selector;
                p.tree(t)
            })
        })
    }

    /// Whether `selector`, in a capture-checked file, is the `this` of a class whose self type is
    /// its own type with a capture set, and a case of `cases` has a pattern other than one typed
    /// with that class (`case _` of `self match` in `trait Ops { self: Ops^ => }`).
    fn is_untold_self_match(&mut self, selector: TNodeId, cases: &[TNodeId]) -> bool {
        let selector = match *self.tpd.tree(selector) {
            Tree::Typed { expr, .. } => expr,
            _ => selector,
        };
        let Some(tpe) = self.tpd.get(selector).tpe else { return false };
        let Type::This(c) = *self.u.ty(tpe) else { return false };
        let Some(class) = self.u.class_symbol(c) else { return false };
        if !self.capturing_selves.contains(&class) || self.u.given_self_type(class).is_some() {
            return false;
        }
        cases.iter().any(|&case| {
            let Tree::CaseDef { pat, .. } = *self.tpd.tree(case) else { return true };
            let pat = match *self.tpd.tree(pat) {
                Tree::Bind { body, .. } => body,
                _ => pat,
            };
            let Tree::Typed { tpt, .. } = *self.tpd.tree(pat) else { return true };
            self.u.class_symbol(self.tpd.tpe(tpt)) != Some(class)
        })
    }

    /// A case, its pattern pickled with the type trees in it, of its match's ascribed selector,
    /// and its match's selector.
    pub fn case_def(&mut self, pat: TNodeId, guard: Option<TNodeId>, body: TNodeId) -> Result<(), Refusal> {
        let selector = self.scrutinee.take();
        let ascribed: Vec<TNodeId> = (selector.map(|s| self.tpd.tree(s)))
            .and_then(|t| if let Tree::Typed { tpt, .. } = *t { Some(tpt) } else { None })
            .into_iter()
            .collect();
        let tpts = self.annotated_tpts(pat);
        self.with_length(CASEDEF, &mut |p| {
            let outer = std::mem::replace(&mut p.pattern_selector, selector);
            let pickled = p.with_typed_tpts(ascribed.clone(), &mut |p| {
                p.with_typed_tpts(tpts.clone(), &mut |p| p.tree(pat))
            });
            p.pattern_selector = outer;
            pickled?;
            [Some(body), guard].into_iter().flatten().try_for_each(|t| p.tree(t))
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
        let info = self.cleaned_pattern_type(info);
        let (flags, term) = (self.u.symbol(sym).flags.without(CASE), self.u.symbol(sym).kind.is_term());
        let tpts = self.annotated_tpts(body);
        self.with_length(BIND, &mut |p| {
            p.name(&local_name(p.u, sym));
            p.with_typed_tpts(tpts.clone(), &mut |p| p.ty(info, false))?;
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
        let tpe = self.cleaned_pattern_type(tpe);
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

    /// The type trees of the typed patterns in pattern `t`, and the annotated type trees in them.
    pub fn annotated_tpts(&self, t: TNodeId) -> Vec<TNodeId> {
        let mut tpts = match *self.tpd.tree(t) {
            Tree::Typed { tpt, .. } => vec![tpt],
            Tree::Annotated { .. } => vec![t],
            _ => Vec::new(),
        };
        tpts.extend(self.tpd.children(t).into_iter().flat_map(|c| self.annotated_tpts(c)));
        tpts
    }

    /// Pickles `pickle` inside the pattern or super accessor parameter of type trees `tpts`.
    pub fn with_typed_tpts(
        &mut self,
        tpts: Vec<TNodeId>,
        pickle: &mut dyn FnMut(&mut Self) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        self.typed_tpts.push(tpts);
        let pickled = pickle(self);
        self.typed_tpts.pop();
        pickled
    }
}
