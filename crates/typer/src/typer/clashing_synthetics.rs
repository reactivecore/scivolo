// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Synthetic case-class methods that a user's member replaces (dotty's
//! `invalidateIfClashingSynthetic`).

use super::Typer;
use crate::symbols::Completion;
use names::nme;
use syntax::ast::NodeId;
use syntax::ast::flags as f;
use tasty::{SymbolId, Universe};

/// The synthetic methods of a case class or its companion that a definition of the user removes,
/// besides the default getters of `copy`.
const RETRACTABLE: [&str; 4] = [nme::APPLY, nme::UNAPPLY, nme::UNAPPLY_SEQ, nme::COPY];

impl Typer<'_> {
    /// Body statements `stats` of class `cls` without the synthetic methods a member of the user
    /// replaces, which are unentered.
    pub fn without_clashing_synthetics(&mut self, cls: SymbolId, stats: &[NodeId]) -> Vec<NodeId> {
        for d in self.u.symbol(cls).decls.clone() {
            let name = self.u.symbol(d).name.clone();
            if self.syms.flags.get(&d).is_some_and(|fl| fl.is(f::SYNTHETIC))
                && self.is_retractable_name(cls, &name)
            {
                self.complete(d);
            }
        }
        let kept = |s: &NodeId| !self.syms.by_tree.get(s).is_some_and(|d| self.syms.retracted.contains(d));
        stats.iter().copied().filter(kept).collect()
    }

    /// Members `syms` with the synthetic methods a member of the user replaces in their place,
    /// their classes and they completed first.
    pub fn without_clashing(&mut self, mut syms: Vec<SymbolId>) -> Vec<SymbolId> {
        let mut replacing = Vec::new();
        for &s in &syms {
            let Some(cls) = self.u.symbol(s).owner else { continue };
            let name = self.u.symbol(s).name.clone();
            if !self.syms.flags.get(&s).is_some_and(|fl| fl.is(f::SYNTHETIC))
                || !self.is_retractable_name(cls, &name)
            {
                continue;
            }
            self.complete(cls);
            match self.syms.completions.get(&s) {
                Some(Completion::Pending { .. }) => self.complete(s),
                Some(Completion::Completing(_)) => {}
                _ => self.invalidate_if_clashing(s),
            }
            if self.syms.retracted.contains(&s) {
                replacing.extend(self.u.decls_named(cls, &name));
            }
        }
        syms.extend(replacing.into_iter().filter(|r| !syms.contains(r)).collect::<Vec<_>>());
        syms.retain(|s| !self.syms.retracted.contains(s));
        syms
    }

    /// Unenters `d`, a completed synthetic method of a case class or the class of its companion,
    /// if another member of the same name and a matching type defines it, or a concrete member a
    /// parent gives (dotty's `invalidateIfClashingSynthetic`).
    pub fn invalidate_if_clashing(&mut self, d: SymbolId) {
        let Some(cls) = self.u.symbol(d).owner else { return };
        let synthetic = self.syms.flags.get(&d).is_some_and(|fl| fl.is(f::SYNTHETIC));
        let name = self.u.symbol(d).name.clone();
        if !synthetic || !self.u.is_class(cls) || !self.is_retractable_name(cls, &name) {
            return;
        }
        let site = crate::defn::type_ref(self.u, cls);
        let mut clashes = false;
        let others: Vec<SymbolId> = self.u.decls_named(cls, &name).into_iter().filter(|&o| o != d).collect();
        for other in others {
            if matches!(self.syms.completions.get(&other), Some(Completion::Completing(_))) {
                continue;
            }
            self.complete(other);
            clashes |= self.u.matches(site, d, other);
        }
        let parents: Vec<SymbolId> = self.u.linearization(cls).into_iter().skip(1).collect();
        for p in parents {
            for m in self.u.decls_named(p, &name) {
                clashes |= self.is_concrete(m) && self.u.matches(site, d, m);
            }
        }
        if clashes {
            self.u.unenter(d);
            self.syms.retracted.insert(d);
        }
    }

    /// Whether a synthetic method `name` of class `cls` gives way to a member of the user: `cls`
    /// is a case class or its companion's class (dotty's `isRetractableCaseClassMethodName`).
    pub fn is_retractable_name(&mut self, cls: SymbolId, name: &str) -> bool {
        (RETRACTABLE.contains(&name) || name.starts_with("copy$default$"))
            && is_case_class_or_companion(self.u, cls)
    }
}

fn is_case_class_or_companion(u: &mut Universe, cls: SymbolId) -> bool {
    let class = match u.is_module_class(cls) {
        true => u.companion_class(cls),
        false => Some(cls),
    };
    class.is_some_and(|c| u.symbol(c).flags.has(tasty::tags::CASE))
}
