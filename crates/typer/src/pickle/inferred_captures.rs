// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/cc/CaptureOps.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Capture sets in inferred type trees of capture-checked files: each emptied (dotty's
//! `CleanupRetains` of an inferred `TypeTree` in PostTyper), an overriding member's kept
//! (`explicifyTpt`).

use super::capture_risks::{CaptureRisks, Risk};
use super::captured_exprs::{Captured, captured};
use super::retaining::emptied;
use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree};
use crate::typer::capture_types::has_captures;
use crate::typer::seen_from::{resolve_class_params, resolve_class_params_keeping_captures};
use names::nme;
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::tags::PRIVATE;
use tasty::{Kind, SymbolId, TypeId, Universe};

/// The capture sets told for the inferred type trees of a file pickled.
#[derive(Default)]
pub struct InferredCaptures {
    /// The risks of the file's scopes, determined at its first definition.
    pub risks: Option<CaptureRisks>,
    /// The risk of the scope pickled now.
    scope: Risk,
    /// The type told for each definition of an inferred type, with its capture sets.
    pub kept_infos: FxHashMap<SymbolId, TypeId>,
    /// The inferred type trees told to have no capture sets.
    pub capture_free: FxHashSet<TNodeId>,
    /// The closure methods without an expected type, whose right-hand sides tell their results.
    pub free_closures: FxHashSet<SymbolId>,
}

/// Why an inferred type tree whose capture sets are not told is not pickled.
const UNTOLD: Refusal = "an inferred type that dotty's typer may give capture sets not told";

impl TreePickler<'_> {
    fn capture_checked(&self) -> bool {
        self.file.is_some_and(|f| f.attributes.features.capture_checking())
    }

    /// Enters the scope of definition `sym` if it is one; returns the risk of the scope it was in.
    pub fn enter_capture_scope(&mut self, sym: SymbolId) -> Risk {
        if self.capture_checked() && self.inferred.risks.is_none() {
            let risks = CaptureRisks::new(self.u, self.tpd, self.root);
            self.inferred.scope = risks.outer();
            self.inferred.risks = Some(risks);
        }
        let outer = self.inferred.scope;
        if let Some(risk) = self.inferred.risks.as_ref().and_then(|r| r.scope(sym)) {
            self.inferred.scope = risk;
        }
        outer
    }

    /// Leaves a scope for the one it was in, of risk `outer`.
    pub fn leave_capture_scope(&mut self, outer: Risk) {
        self.inferred.scope = outer;
    }

    /// Refuses inferred type tree `tpt` of type `tpe` if dotty's typer may give it capture sets
    /// the scope's types have, but none were told, a type argument's by its call either: unless
    /// `tpe` is a primitive value class, which only a capture set on a type other than a class's
    /// may become.
    pub fn check_inferred_captures(&mut self, tpt: TNodeId, tpe: TypeId) -> Result<(), Refusal> {
        if self.capture_checked() && matches!(self.tpd.tree(tpt), Tree::TypeTree { inferred: true }) {
            if !self.kept_captures.contains_key(&tpt) && has_captures(self.u, tpe) {
                let kept = emptied(self.u, tpe);
                self.kept_captures.insert(tpt, kept);
            }
            return Ok(());
        }
        let scope = self.inferred.scope;
        if !scope.captures
            || !matches!(self.tpd.tree(tpt), Tree::TypeTree { inferred: true })
            || self.told(tpt)
        {
            return Ok(());
        }
        if let Some(call) = self.inferred.risks.as_ref().and_then(|r| r.call_of(tpt)) {
            self.captured_expr(call);
            if self.told(tpt) {
                return Ok(());
            }
        }
        let class = self.u.applied(tpe).is_none().then(|| self.u.class_symbol(tpe)).flatten();
        let primitive = tpe == tasty::NOTHING
            || class.is_some_and(|c| tasty::PRIMITIVE_VALUE_CLASSES.contains(&self.u.full_name(c).as_str()));
        match primitive && !scope.on_abstract {
            true => Ok(()),
            false => Err(UNTOLD),
        }
    }

    fn told(&self, tpt: TNodeId) -> bool {
        self.kept_captures.contains_key(&tpt)
            || self.inferred.capture_free.contains(&tpt)
            || self.tpd.captured_type(tpt).is_some()
            || self.inferred.risks.as_ref().is_some_and(|r| r.is_pure_element(tpt))
    }

    /// Refuses a refined type tree of parent `parent` and refinement `member` in a capture-checked
    /// file if the member's type has capture sets and the parent's not, as the function type
    /// parent of `(x: A^) -> B^{x}`, `A^ -> B^` in dotty's.
    pub fn check_refinement_captures(&mut self, parent: TNodeId, member: SymbolId) -> Result<(), Refusal> {
        if !self.capture_checked() {
            return Ok(());
        }
        let info = self.u.info(member);
        let parent = self.tpd.get(parent).tpe.ok_or("a tree without a type")?;
        match has_captures(self.u, info) && !has_captures(self.u, parent) {
            true => Err("a refined type whose parent drops its refinement's capture sets"),
            false => Ok(()),
        }
    }

    /// Records the type inferred type tree `tpt` of definition `sym` of a capture-checked file is
    /// pickled with: the type dotty's typer gives the definition, with each capture set but
    /// `retainsCap`'s empty, `B^{}` of `y` in `def g(b: B^{a}) = { val y = b; .. }`, or as it is
    /// if `sym` overrides a member. That type is the one the typer recorded (a function value's
    /// parameter's), else an overridden member's, else unless `sym` is a closure's method with an
    /// expected type the type of right-hand side `rhs` (`captured_expr`).
    pub fn empty_captures(&mut self, sym: SymbolId, tpt: TNodeId, rhs: Option<TNodeId>) {
        if !self.capture_checked() {
            return;
        }
        let (Tree::TypeTree { inferred: true }, Some(tpe)) = (self.tpd.tree(tpt), self.tpd.get(tpt).tpe)
        else {
            return;
        };
        let overriding = !overridden_infos(self.u, sym).is_empty();
        if !overriding {
            return;
        }
        let closure = self.u.symbol(sym).name == nme::ANON_FUN && !self.inferred.free_closures.contains(&sym);
        let from = match self.tpd.captured_type(tpt) {
            Some(t) => Some(captured(self.u, t)),
            None if overriding => self.inherited(sym),
            None if closure => None,
            None => rhs.and_then(|r| self.captured_expr(r)),
        };
        let from = match from {
            Some(Captured::Ours) => {
                self.inferred.capture_free.insert(tpt);
                let info = self.u.info(sym);
                self.inferred.kept_infos.insert(sym, info);
                return;
            }
            Some(Captured::With(t)) => t,
            None => return,
        };
        let kept = if overriding { from } else { emptied(self.u, from) };
        let kept = resolve_class_params_keeping_captures(self.u, kept);
        if resolve_class_params(self.u, kept) != tpe {
            return;
        }
        let info = self.u.info(sym);
        if self.u.method(info).is_none() && self.u.poly(info).is_none() {
            self.inferred.kept_infos.insert(sym, from);
        }
        self.kept_captures.insert(tpt, kept);
        if let Some(class) = self.u.symbol(sym).owner.filter(|&o| overriding && self.u.is_class(o)) {
            self.capture_classes.insert(tpt, class);
        }
    }

    /// The type of the members `sym`, a member without parameters, overrides, if all have one
    /// (dotty's `inherited` of `inferredResultType`).
    fn inherited(&mut self, sym: SymbolId) -> Option<Captured> {
        let own = self.u.info(sym);
        if self.u.method(own).is_some() || self.u.poly(own).is_some() {
            return None;
        }
        let infos = overridden_infos(self.u, sym);
        let first = *infos.first()?;
        if infos.iter().any(|&i| i != first) {
            return None;
        }
        let t = self.u.by_name(first).unwrap_or(first);
        if self.u.method(t).is_some() || self.u.poly(t).is_some() {
            return None;
        }
        Some(captured(self.u, t))
    }
}

/// The infos, as seen from its class, of the members that member `sym` overrides: the non-private
/// term members of its name of its class's base classes (dotty's `allOverriddenSymbols`).
pub fn overridden_infos(u: &mut Universe, sym: SymbolId) -> Vec<TypeId> {
    let Some(cls) = u.symbol(sym).owner.filter(|&o| u.symbol(o).kind == Kind::Class) else {
        return Vec::new();
    };
    let this = u.this_type(cls);
    let name = u.symbol(sym).name.clone();
    let mut infos = Vec::new();
    for base in u.linearization(cls).into_iter().skip(1) {
        let members: Vec<SymbolId> = (u.symbol(base).decls.iter().copied())
            .filter(|&m| u.symbol(m).name == name && u.symbol(m).kind.is_term())
            .filter(|&m| !u.symbol(m).flags.has(PRIVATE))
            .collect();
        for m in members {
            infos.push(u.member_info(this, m));
        }
    }
    infos
}
