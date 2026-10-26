// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! The type a local class is approximated by when avoiding it (dotty's `classBound`).

use super::Typer;
use super::avoid_map::Avoided;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The intersection of local class `c`'s parents refined by each type member narrowing an
    /// inherited one, or by any member if a parent is a `Selectable`; `None` for a polymorphic
    /// class, a term member narrowing an inherited one, or a refinement referring to `c` or to
    /// one of the local symbols being expanded, `visiting`.
    pub fn local_class_bound(&mut self, c: SymbolId, visiting: &[SymbolId]) -> Option<TypeId> {
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
            let is_type = self.u.is_type_symbol(d);
            let name = self.u.symbol(d).name.clone();
            if !is_type && !self.type_members(parent, &name).is_empty() {
                return None;
            }
            let info = if is_type { self.type_info(d) } else { self.term_info(d) };
            let avoided = Avoided { locals: &missing, opaque: &[], visiting };
            let info = self.avoid_at(info, 1, avoided)?.hi();
            if self.refers_to(info, &[c]) {
                return None;
            }
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
            let Type::Bounds(lo, hi) = *self.u.types().get(theirs) else { return false };
            let (own_lo, own_hi) = match *self.u.types().get(own) {
                Type::Alias(a) => (a, a),
                Type::Bounds(l, h) => (l, h),
                _ => return theirs != own,
            };
            return !(self.u.is_subtype(own_lo, lo) && self.u.is_subtype(hi, own_hi));
        }
        let info = self.u.info(d);
        let poly = matches!(self.u.types().get(info), Type::Poly { .. });
        let own = if poly { info } else { self.final_result(info) };
        let class = self.u.intern_type(Type::Sym(None, c));
        let this = self.u.intern_type(Type::This(class));
        inherited.into_iter().any(|m| {
            let info = self.u.member_info(this, m);
            let theirs = if poly { info } else { self.final_result(info) };
            let theirs = self.own_type_members(c, this, theirs);
            let narrower = own == theirs || self.u.is_subtype(own, theirs);
            let wider = own == theirs || self.u.is_subtype(theirs, own);
            let unsure = self.refers_to(own, &[c]) || self.refers_to(theirs, &[c]);
            !(narrower && wider) && (poly || narrower || unsure)
        })
    }

    /// `t` with each type member selected on `this` of class `c` replaced by the member of `c`
    /// of that name that overrides it.
    fn own_type_members(&mut self, c: SymbolId, this: TypeId, t: TypeId) -> TypeId {
        self.u.map_type(t, &mut |u, part| {
            let Type::Sym(Some(pre), m) = *u.types().get(part) else { return None };
            if pre != this || u.symbol(m).kind != Kind::Type {
                return None;
            }
            let name = u.symbol(m).name.clone();
            let decls = &u.symbol(c).decls;
            let own = decls
                .iter()
                .copied()
                .find(|&d| u.symbol(d).kind == Kind::Type && u.symbol(d).name == name)?;
            Some(u.intern_type(Type::Sym(Some(pre), own)))
        })
    }

    /// Whether member `d` of class `c` is the `apply` of a polymorphic function value.
    fn is_poly_function_apply(&mut self, c: SymbolId, d: SymbolId) -> bool {
        self.u.symbol(d).name == "apply"
            && self.resolve_full_name("scala.PolyFunction", true).is_some_and(|p| self.u.derives_from(c, p))
    }

    /// The info of term member `d` as a refinement gives it: a parameterless method's by-name.
    fn term_info(&mut self, d: SymbolId) -> TypeId {
        let info = self.u.info(d);
        let method =
            matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. } | Type::ByName(_));
        if self.u.symbol(d).kind == Kind::Def && !method {
            return self.u.intern_type(Type::ByName(info));
        }
        info
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
