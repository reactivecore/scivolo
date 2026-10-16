//! The type a local class is approximated by when avoiding it (dotty's `classBound`).

use super::Typer;
use super::avoid_map::Avoided;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The intersection of local class `c`'s parents refined by each type member narrowing an
    /// inherited one, or by any member if a parent is a `Selectable`; `None` for a polymorphic
    /// class, a term member that needs a refinement, or a refinement referring to `c`.
    pub fn local_class_bound(&mut self, c: SymbolId) -> Option<TypeId> {
        if !self.u.type_params(c).is_empty() {
            return None;
        }
        let parents = self.u.parents(c);
        let parent = parents.into_iter().fold(tasty::ANY, |acc, p| self.u.glb(acc, p));
        let selectable =
            self.resolve_full_name("scala.Selectable", true).is_some_and(|s| self.u.derives_from(c, s));
        let decls = self.u.symbol(c).decls.clone();
        let (refinable, missing): (Vec<SymbolId>, Vec<SymbolId>) = decls.into_iter().partition(|&d| {
            let s = self.u.symbol(d);
            !s.flags.has(tasty::tags::PRIVATE) && s.name != "<init>" && s.kind != Kind::Class
        });
        let mut bound = parent;
        for d in refinable {
            if !self.needs_refinement(c, d, parent, selectable) {
                continue;
            }
            if !self.u.is_type_symbol(d) {
                return None;
            }
            let info = self.type_info(d);
            let avoided = Avoided { locals: &missing, opaque: &[] };
            let info = self.avoid_at(info, 1, avoided)?.hi();
            if self.refers_to(info, &[c]) {
                return None;
            }
            let name = self.u.symbol(d).name.clone();
            bound = self.u.intern_type(Type::Refined(bound, name, info));
        }
        Some(bound)
    }

    /// Whether member `d` of class `c` narrows the member of `parent` it overrides, or overrides
    /// none in a `Selectable`.
    fn needs_refinement(&mut self, c: SymbolId, d: SymbolId, parent: TypeId, selectable: bool) -> bool {
        let name = self.u.symbol(d).name.clone();
        let is_type = self.u.is_type_symbol(d);
        let site = crate::defn::type_ref(self.u, c);
        let inherited: Vec<SymbolId> = self
            .type_members(parent, &name)
            .into_iter()
            .filter(|&m| self.u.is_type_symbol(m) == is_type && (is_type || self.same_member(site, m, d)))
            .collect();
        if self.is_poly_function_apply(c, d) {
            return true;
        }
        if inherited.is_empty() {
            return selectable;
        }
        if is_type {
            let own = self.type_info(d);
            let theirs = self.u.member_info(parent, inherited[0]);
            return matches!(self.u.types().get(theirs), Type::Bounds(..)) && theirs != own;
        }
        let info = self.u.info(d);
        let own = self.final_result(info);
        let poly = matches!(self.u.types().get(info), Type::Poly { .. });
        inherited.into_iter().any(|m| {
            let info = self.u.member_info(parent, m);
            let theirs = self.final_result(info);
            let narrower = self.u.is_subtype(own, theirs);
            let wider = self.u.is_subtype(theirs, own);
            (poly || narrower) && !(narrower && wider)
        })
    }

    /// Whether member `d` of class `c` is the `apply` of a polymorphic function value.
    fn is_poly_function_apply(&mut self, c: SymbolId, d: SymbolId) -> bool {
        self.u.symbol(d).name == "apply"
            && self.resolve_full_name("scala.PolyFunction", true).is_some_and(|p| self.u.derives_from(c, p))
    }

    /// The info of type member `d` as a refinement gives it: an alias or bounds.
    fn type_info(&mut self, d: SymbolId) -> TypeId {
        let info = self.u.info(d);
        match self.u.types().get(info) {
            Type::Alias(_) | Type::Bounds(..) => info,
            _ => self.u.intern_type(Type::Alias(info)),
        }
    }
}
