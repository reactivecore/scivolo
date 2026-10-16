use super::Typer;
use crate::symbols::Completion;
use syntax::ast::Span;
use tasty::SymbolId;

/// The classes whose members a package object does not contribute (dotty's `topClasses`).
const TOP_CLASSES: [&str; 4] = ["scala.Any", "scala.Matchable", "java.lang.Object", "scala.AnyVal"];

impl Typer<'_> {
    /// The members named `name` of package `p`: its own and those of its package objects, the
    /// ones the unit defines among them completed first; of package `scala` its own if it has
    /// some (dotty's `computeMembersNamed`).
    pub fn package_members(&mut self, p: SymbolId, name: &str) -> Vec<SymbolId> {
        let named: Vec<SymbolId> =
            self.u.symbol(p).decls.iter().copied().filter(|&d| self.is_named_package_object(d)).collect();
        for o in named {
            if matches!(self.syms.completions.get(&o), Some(Completion::Pending { .. })) {
                self.complete(o);
            }
        }
        let mut members = self.u.decl(p, name).unwrap_or_default();
        if !members.is_empty() && self.u.full_name(p) == "scala" {
            return members;
        }
        let Ok(objects) = self.u.package_objects(p) else {
            self.unsupported_at(Span::NONE, "package object that cannot be read");
            return members;
        };
        for o in objects {
            for m in self.class_members(o, name) {
                if !members.contains(&m) && self.is_package_object_member(o, m) {
                    members.push(m);
                }
            }
        }
        members
    }

    /// Whether member `m` of package object `o` is one its package has: not private, and not
    /// one of a top class.
    fn is_package_object_member(&mut self, o: SymbolId, m: SymbolId) -> bool {
        let owner = self.u.symbol(m).owner;
        let top = owner.is_some_and(|c| TOP_CLASSES.contains(&self.u.full_name(c).as_str()));
        let private = self.u.symbol(m).flags.has(tasty::tags::PRIVATE) && owner != Some(o);
        !top && !private
    }
}
