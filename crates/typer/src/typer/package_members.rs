use super::Typer;
use crate::symbols::Completion;
use tasty::SymbolId;

/// The classes whose members a package object does not contribute (dotty's `topClasses`).
const TOP_CLASSES: [&str; 4] = ["scala.Any", "scala.Matchable", "java.lang.Object", "scala.AnyVal"];

impl Typer<'_> {
    /// The members named `name` of package `p`, those of its package objects that the unit
    /// defines among them once completed; else the non-private members its package objects
    /// inherit from classes other than the top classes (dotty's `computeMembersNamed`).
    pub fn package_members(&mut self, p: SymbolId, name: &str) -> Vec<SymbolId> {
        let objects: Vec<SymbolId> =
            self.u.symbol(p).decls.iter().copied().filter(|&d| self.is_named_package_object(d)).collect();
        for &o in &objects {
            if matches!(self.syms.completions.get(&o), Some(Completion::Pending { .. })) {
                self.complete(o);
            }
        }
        let direct = self.u.decl(p, name).unwrap_or_default();
        if !direct.is_empty() {
            return direct;
        }
        let mut inherited = Vec::new();
        for o in self.u.package_objects(p).unwrap_or(objects) {
            for m in self.class_members(o, name) {
                let owner = self.u.symbol(m).owner;
                let top = owner.is_some_and(|c| TOP_CLASSES.contains(&self.u.full_name(c).as_str()));
                if owner != Some(o) && !top && !self.u.symbol(m).flags.has(tasty::tags::PRIVATE) {
                    inherited.push(m);
                }
            }
        }
        inherited
    }
}
