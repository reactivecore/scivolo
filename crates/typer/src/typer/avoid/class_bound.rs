// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! The type a local class is approximated by when avoiding it (dotty's `classBound`).

use super::Avoid;
use crate::typer::Typer;
use tasty::{Kind, SymbolId, Type, TypeId, Universe, nme};

impl Avoid<'_> {
    /// Determines the type that stands for local class `c` outside its block: for
    /// `class C extends A with B { def f: Int }` the type `A & B` refined by the members of `C`
    /// that narrow an inherited one, or by all of them if a parent is a `Selectable`, a member
    /// mentioning `C` itself through a recursive `this`. Gives up on a generic local class and on
    /// a class whose members mention a local definition already being expanded.
    pub fn class_bound(self, t: &mut Typer, c: SymbolId) -> Option<TypeId> {
        if !t.u.type_params(c).is_empty() {
            return None;
        }
        let parents = t.u.parents(c);
        let parent = parents.into_iter().fold(tasty::ANY, |acc, p| t.u.glb(acc, p));
        let selectable =
            t.resolve_full_name(crate::defn::SELECTABLE, true).is_some_and(|s| t.u.derives_from(c, s));
        let decls = t.u.symbol(c).decls.clone();
        let (refinable, missing): (Vec<SymbolId>, Vec<SymbolId>) = decls.into_iter().partition(|&d| {
            let s = t.u.symbol(d);
            !s.flags.has(tasty::tags::PRIVATE) && s.name != nme::CONSTRUCTOR && s.kind != Kind::Class
        });
        let mut bound = parent;
        for d in refinable {
            if !needs_refinement(t, c, d, parent, selectable) {
                continue;
            }
            let is_type = t.u.is_type_symbol(d);
            let name = t.u.interned_name(d);
            let info = if is_type { type_info(t.u, d) } else { term_info(t.u, d) };
            let avoided = Avoid { visiting: self.visiting, ..Avoid::new(&missing, &[]) };
            let info = avoided.at(t, info, 1)?.hi();
            bound = t.u.intern_type(Type::Refined(bound, name, info));
        }
        Some(closed_over(t, c, bound))
    }
}

/// `tpe` with references to class `c` and its `this` replaced by the `this` of a recursive type
/// around it, `tpe` itself if it has none (dotty's `RecType.closeOver`).
fn closed_over(t: &mut Typer, c: SymbolId, tpe: TypeId) -> TypeId {
    let binder = t.syms.fresh_binder();
    let rec_this = t.u.intern_type(Type::RecThis(binder));
    let mut found = false;
    let closed = t.u.map_type(tpe, &mut |u, part| {
        let ours = match *u.ty(part) {
            Type::Sym(_, s) => s == c,
            Type::This(x) => u.class_symbol(x) == Some(c),
            _ => false,
        };
        found |= ours;
        ours.then_some(rec_this)
    });
    if found { t.u.intern_type(Type::Rec(binder, closed)) } else { tpe }
}

/// Whether member `d` of class `c` narrows the member of `parent` it overrides, or overrides
/// none in a `Selectable`.
fn needs_refinement(t: &mut Typer, c: SymbolId, d: SymbolId, parent: TypeId, selectable: bool) -> bool {
    let name = t.u.symbol(d).name.clone();
    let is_type = t.u.is_type_symbol(d);
    let site = crate::defn::type_ref(t.u, c);
    let inherited: Vec<SymbolId> = t
        .type_members(parent, &name)
        .into_iter()
        .filter(|&m| t.u.is_type_symbol(m) == is_type && (is_type || t.same_member(site, m, d)))
        .collect();
    if is_poly_function_apply(t, c, d) {
        return true;
    }
    if inherited.is_empty() {
        return selectable;
    }
    if is_type {
        let own = type_info(t.u, d);
        let theirs = t.u.member_info(parent, inherited[0]);
        let Some((lo, hi)) = t.u.bounds(theirs) else { return false };
        let (own_lo, own_hi) = match *t.u.ty(own) {
            Type::Alias(a) => (a, a),
            Type::Bounds(l, h) => (l, h),
            _ => return theirs != own,
        };
        return !(t.u.is_subtype(own_lo, lo) && t.u.is_subtype(hi, own_hi));
    }
    let info = t.u.info(d);
    let poly = matches!(t.u.ty(info), Type::Poly { .. });
    let own = if poly { info } else { t.final_result(info) };
    let class = t.u.intern_type(Type::Sym(None, c));
    let this = t.u.intern_type(Type::This(class));
    inherited.into_iter().any(|m| {
        let info = t.u.member_info(this, m);
        let theirs = if poly { info } else { t.final_result(info) };
        let theirs = own_type_members(t.u, c, this, theirs);
        let narrower = own == theirs || t.u.is_subtype(own, theirs);
        let wider = own == theirs || t.u.is_subtype(theirs, own);
        let unsure = t.refers_to(own, &[c]) || t.refers_to(theirs, &[c]);
        !(narrower && wider) && (poly || narrower || unsure)
    })
}

/// `tpe` with each type member selected on `this` of class `c` replaced by the member of `c` of
/// that name that overrides it.
fn own_type_members(u: &mut Universe, c: SymbolId, this: TypeId, tpe: TypeId) -> TypeId {
    u.map_type(tpe, &mut |u, part| {
        let Some((Some(pre), m)) = u.sym_ref(part) else { return None };
        if pre != this || u.symbol(m).kind != Kind::Type {
            return None;
        }
        let name = u.symbol(m).name.clone();
        let decls = &u.symbol(c).decls;
        let own =
            decls.iter().copied().find(|&d| u.symbol(d).kind == Kind::Type && u.symbol(d).name == name)?;
        Some(u.intern_type(Type::Sym(Some(pre), own)))
    })
}

/// Whether member `d` of class `c` is the `apply` of a polymorphic function value.
fn is_poly_function_apply(t: &mut Typer, c: SymbolId, d: SymbolId) -> bool {
    t.u.symbol(d).name == nme::APPLY
        && t.resolve_full_name(crate::defn::POLY_FUNCTION, true).is_some_and(|p| t.u.derives_from(c, p))
}

/// The info of term member `d` as a refinement gives it: a parameterless method's by-name.
fn term_info(u: &mut Universe, d: SymbolId) -> TypeId {
    let info = u.info(d);
    let method = matches!(u.ty(info), Type::Method { .. } | Type::Poly { .. } | Type::ByName(_));
    if u.symbol(d).kind == Kind::Def && !method {
        return u.intern_type(Type::ByName(info));
    }
    info
}

/// The info of type member `d` as a refinement gives it: an alias or bounds.
fn type_info(u: &mut Universe, d: SymbolId) -> TypeId {
    let info = u.info(d);
    match *u.ty(info) {
        Type::Alias(_) | Type::Bounds(..) => info,
        _ => u.intern_type(Type::Alias(info)),
    }
}
