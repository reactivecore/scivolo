// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The implicits an implicit scope takes from the objects and values on the prefix paths of its
//! parts (dotty's `addPath`).

use super::super::Typer;
use super::super::type_ops::TypeOps;
use super::candidates::{Candidate, implicit_members};
use super::parts::is_excluded;
use super::scope::ImplicitScope;
use tasty::{SymbolId, Type, TypeId, Universe};

/// How many bounds and intersections finding the classes of a path's type passes at most.
const MAX_BOUND_STEPS: u32 = 16;

impl ImplicitScope {
    /// Adds the implicit members of `p` if it is a static object other than a package object, or
    /// the `this` of one (dotty's `addPath` of a module).
    pub fn add_static_module(&mut self, t: &mut Typer<'_>, p: TypeId) {
        let s = match *t.u.types().get(p) {
            Type::This(c) => match t.u.class_symbol(c).filter(|&c| t.u.is_module_class(c)) {
                Some(c) => t.u.source_module(c),
                None => return,
            },
            _ => t.types().referenced(p),
        };
        if let Some(s) = s
            && t.u.is_module(s)
            && t.u.is_static_symbol(s)
            && !is_excluded(t.u, s)
            && let Some(class) = t.u.module_class(s)
        {
            let module_ref = crate::defn::type_ref(t.u, s);
            self.add_members(t, class, module_ref);
        }
    }

    /// Adds the implicit members of `class` selected from path `prefix`, its own private ones
    /// included, and `prefix`.
    pub fn add_members(&mut self, t: &mut Typer<'_>, class: SymbolId, prefix: TypeId) {
        let key = static_form(t.u, prefix);
        for sym in implicit_members(t, class, true) {
            let same = |u: &mut Universe, c: &Candidate| {
                c.sym == sym && c.prefix.map(|p| static_form(u, p)) == Some(key)
            };
            if !self.implicits.iter().any(|c| same(t.u, c)) {
                self.implicits.push(Candidate { sym, prefix: Some(prefix) });
            }
        }
        if let Type::Sym(p, sym) = *t.u.types().get(key)
            && !self.containers.contains(&Candidate { sym, prefix: p })
        {
            self.containers.push(Candidate { sym, prefix: p });
        }
    }

    /// Adds the implicit members of the stable values that are not static objects on path `p`
    /// (dotty's `addPath`).
    pub fn add_path(&mut self, t: &mut Typer<'_>, p: TypeId) -> Result<(), &'static str> {
        let outer = match *t.u.types().get(p) {
            Type::Sym(outer, _) => outer,
            Type::TermRef(outer, _) => Some(outer),
            _ => return Ok(()),
        };
        let s = t.types().referenced(p).ok_or("implicit scope through an unresolved path")?;
        if !t.u.symbol(s).is_term() || is_excluded(t.u, s) || t.u.is_static_symbol(s) && t.u.is_module(s) {
            return Ok(());
        }
        t.complete(s);
        let info = t.u.info(s);
        if matches!(t.u.types().get(info), Type::Sym(_, x) if t.u.symbol(*x).is_term()) {
            return self.add_path(t, info);
        }
        let widened = t.u.widen(p);
        let classes = bound_classes(&mut t.types(), widened, MAX_BOUND_STEPS);
        for class in classes.ok_or("implicit scope through a value of no class type")? {
            self.add_members(t, class, p);
        }
        match outer {
            Some(outer) => self.add_path(t, outer),
            None => Ok(()),
        }
    }
}

/// Reference `t` to a member of a package through the package, as `type_ref` refers to it.
fn static_form(u: &mut Universe, t: TypeId) -> TypeId {
    match *u.types().get(t) {
        Type::Sym(Some(p), s) if matches!(u.types().get(p), Type::Package(_)) => crate::defn::type_ref(u, s),
        _ => t,
    }
}

/// The classes of `tpe`, through the upper bounds of abstract types and intersections; `None`
/// if a part of `tpe` has none.
fn bound_classes(ty: &mut TypeOps<'_>, tpe: TypeId, steps: u32) -> Option<Vec<SymbolId>> {
    if let Some(c) = ty.u.class_symbol(tpe) {
        return Some(vec![c]);
    }
    let steps = steps.checked_sub(1)?;
    match ty.u.types().get(tpe).clone() {
        Type::TypeRef(p, name) => match ty.u.refinement_named(p, &name).map(|i| ty.u.types().get(i)) {
            Some(&Type::Bounds(_, hi) | &Type::Alias(hi)) => bound_classes(ty, hi, steps),
            _ => None,
        },
        Type::Refined(parent, ..) => bound_classes(ty, parent, steps),
        Type::And(a, b) => {
            let mut classes = bound_classes(ty, a, steps)?;
            classes.extend(bound_classes(ty, b, steps)?);
            Some(classes)
        }
        Type::Sym(prefix, s) if ty.u.is_type_symbol(s) => {
            let info = ty.refined_info(prefix, s);
            let hi = match *ty.u.types().get(info) {
                Type::Bounds(_, hi) | Type::Alias(hi) => hi,
                _ => return None,
            };
            let info = match (prefix, ty.u.symbol(s).owner) {
                (Some(p), Some(o)) => ty.u.as_seen_from(hi, p, o),
                _ => hi,
            };
            bound_classes(ty, info, steps)
        }
        Type::Sym(..) | Type::TermRef(..) | Type::Constant(..) | Type::Str(_) => {
            let widened = ty.u.widen(tpe);
            if widened == tpe { None } else { bound_classes(ty, widened, steps) }
        }
        _ => None,
    }
}
