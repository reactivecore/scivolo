//! Refinements of a local class's parents when avoiding it (dotty's `classBound`).

use super::Typer;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether local class `c` with parents `parent` declares a member that dotty's `classBound`
    /// may add as a refinement: a type member, a member narrowing an inherited one or polymorphic
    /// and overriding one, or a member of a `Selectable`.
    pub fn needs_refinement(&mut self, c: SymbolId, parent: TypeId) -> bool {
        let selectable =
            self.resolve_full_name("scala.Selectable", true).is_some_and(|s| self.u.derives_from(c, s));
        let site = crate::defn::type_ref(self.u, c);
        for d in self.u.symbol(c).decls.clone() {
            let sym = self.u.symbol(d);
            if sym.flags.has(tasty::tags::PRIVATE) || sym.name == "<init>" || sym.kind == tasty::Kind::Class {
                continue;
            }
            if self.u.is_type_symbol(d) {
                return true;
            }
            let name = sym.name.clone();
            let info = self.u.info(d);
            if matches!(self.u.types().get(info), Type::Poly { .. })
                && !self.type_members(parent, &name).is_empty()
            {
                return true;
            }
            let inherited: Vec<SymbolId> = self
                .type_members(parent, &name)
                .into_iter()
                .filter(|&m| !self.u.is_type_symbol(m) && self.same_member(site, m, d))
                .collect();
            if inherited.is_empty() {
                if selectable {
                    return true;
                }
                continue;
            }
            let own = self.final_result(info);
            for m in inherited {
                let info = self.u.member_info(parent, m);
                let theirs = self.final_result(info);
                if self.u.is_subtype(own, theirs) && !self.u.is_subtype(theirs, own) {
                    return true;
                }
            }
        }
        false
    }
}
