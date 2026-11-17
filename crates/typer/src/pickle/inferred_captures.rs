// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/cc/CaptureOps.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Capture sets in inferred type trees of capture-checked files, as the typer gives them (ADR
//! 0126): each emptied (dotty's `CleanupRetains` of an inferred `TypeTree` in PostTyper), an
//! overriding member's kept (`explicifyTpt`).

use super::retaining::emptied;
use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree};
use crate::typer::capture_types::has_captures;
use tasty::tags::PRIVATE;
use tasty::{Kind, SymbolId, TypeId, Universe};

impl TreePickler<'_> {
    fn capture_checked(&self) -> bool {
        self.file.is_some_and(|f| f.attributes.features.capture_checking())
    }

    /// Records the type inferred type tree `tpt` of type `tpe` of a capture-checked file is
    /// pickled with: `tpe` with each capture set but `retainsCap`'s empty, unless recorded before.
    pub fn empty_inferred_captures(&mut self, tpt: TNodeId, tpe: TypeId) {
        if !self.capture_checked()
            || !matches!(self.tpd.tree(tpt), Tree::TypeTree { inferred: true })
            || self.kept_captures.contains_key(&tpt)
            || !has_captures(self.u, tpe)
        {
            return;
        }
        let kept = emptied(self.u, tpe);
        self.kept_captures.insert(tpt, kept);
    }

    /// Refuses in a capture-checked file `this` as the right-hand side `rhs` of a definition of
    /// inferred type tree `tpt` or as a match selector (`tpt` none), whose capture set dotty's
    /// typer may take from the class (`var curr = this` of a class with mutable fields).
    pub fn check_this_captures(&mut self, tpt: Option<TNodeId>, e: Option<TNodeId>) -> Result<(), Refusal> {
        let inferred = tpt.is_none_or(|t| matches!(self.tpd.tree(t), Tree::TypeTree { inferred: true }));
        match self.capture_checked()
            && inferred
            && e.is_some_and(|e| matches!(self.tpd.tree(e), Tree::This { .. }))
        {
            true => Err("an inferred type of `this`, which dotty's typer may give a capture set"),
            false => Ok(()),
        }
    }

    /// Refuses a refined type tree of refinement `member` in a capture-checked file if the
    /// member's type has capture sets, whose parent dotty's typer derives from them: the function
    /// type parent of `(x: A^) -> B^{x}`.
    pub fn check_refinement_captures(&mut self, member: SymbolId) -> Result<(), Refusal> {
        if !self.capture_checked() {
            return Ok(());
        }
        let info = self.u.info(member);
        match has_captures(self.u, info) {
            true => Err("a refined type whose parent drops its refinement's capture sets"),
            false => Ok(()),
        }
    }

    /// Records the type of inferred type tree `tpt` of member `sym` of a capture-checked file
    /// as it is, capture sets not emptied, if `sym` overrides a member (dotty's `explicifyTpt`
    /// makes the tree not inferred); its sets may name `sym`'s parameters.
    pub fn keep_overriding_captures(&mut self, sym: SymbolId, tpt: TNodeId) {
        if !self.capture_checked() || overridden_infos(self.u, sym).is_empty() {
            return;
        }
        let (Tree::TypeTree { inferred: true }, Some(tpe)) = (self.tpd.tree(tpt), self.tpd.get(tpt).tpe)
        else {
            return;
        };
        if !has_captures(self.u, tpe) {
            return;
        }
        self.kept_captures.insert(tpt, tpe);
        self.capture_defs.insert(tpt, sym);
        if let Some(class) = self.u.symbol(sym).owner.filter(|&o| self.u.is_class(o)) {
            self.capture_classes.insert(tpt, class);
        }
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
