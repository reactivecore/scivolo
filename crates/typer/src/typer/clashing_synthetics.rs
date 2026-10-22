// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Synthetic case-class methods that a user's member replaces (dotty's
//! `invalidateIfClashingSynthetic`).

use super::Typer;
use rustc_hash::FxHashSet;
use syntax::ast::NodeId;
use syntax::ast::flags as f;
use tasty::SymbolId;

/// The synthetic methods of a case class or its companion that a definition of the user removes,
/// besides the default getters of `copy`.
const RETRACTABLE: [&str; 4] = ["apply", "unapply", "unapplySeq", "copy"];

impl Typer<'_> {
    /// Body statements `stats` of class `cls` without the synthetic methods a member of the user
    /// replaces, which are unentered.
    pub fn without_clashing_synthetics(&mut self, cls: SymbolId, stats: &[NodeId]) -> Vec<NodeId> {
        let retracted = self.invalidate_clashing_synthetics(cls);
        let kept = |s: &NodeId| !self.syms.by_tree.get(s).is_some_and(|d| retracted.contains(d));
        stats.iter().copied().filter(kept).collect()
    }

    /// Unenters the synthetic methods of class `cls`, a case class or the class of its companion,
    /// that another member of the same name and a matching type defines, or a concrete member a
    /// parent gives; the methods unentered.
    fn invalidate_clashing_synthetics(&mut self, cls: SymbolId) -> FxHashSet<SymbolId> {
        let mut out = FxHashSet::default();
        let site = crate::defn::type_ref(self.u, cls);
        let decls = self.u.symbol(cls).decls.clone();
        for &d in &decls {
            let synthetic = self.syms.flags.get(&d).is_some_and(|fl| fl.is(f::SYNTHETIC));
            let name = self.u.symbol(d).name.clone();
            if !synthetic || !self.is_retractable_name(cls, &name) {
                continue;
            }
            self.complete(d);
            let mut clashes = false;
            let others: Vec<SymbolId> =
                decls.iter().copied().filter(|&o| o != d && self.u.symbol(o).name == name).collect();
            for other in others {
                self.complete(other);
                clashes |= self.u.matches(site, d, other);
            }
            let parents: Vec<SymbolId> = self.u.linearization(cls).into_iter().skip(1).collect();
            for p in parents {
                for m in self.u.decl(p, &name).unwrap_or_default() {
                    clashes |= self.is_concrete(m) && self.u.matches(site, d, m);
                }
            }
            if clashes {
                self.u.unenter(d);
                self.syms.retracted.insert(d);
                out.insert(d);
            }
        }
        out
    }

    /// Whether a synthetic method `name` of class `cls` gives way to a member of the user: `cls`
    /// is a case class or its companion's class (dotty's `isRetractableCaseClassMethodName`).
    pub fn is_retractable_name(&mut self, cls: SymbolId, name: &str) -> bool {
        (RETRACTABLE.contains(&name) || name.starts_with("copy$default$"))
            && self.is_case_class_or_companion(cls)
    }

    fn is_case_class_or_companion(&mut self, cls: SymbolId) -> bool {
        let class = match self.u.is_module_class(cls) {
            true => self.u.companion_class(cls),
            false => Some(cls),
        };
        class.is_some_and(|c| self.u.symbol(c).flags.has(tasty::tags::CASE))
    }
}
