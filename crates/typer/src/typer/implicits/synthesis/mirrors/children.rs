// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//! Whether the children of a sealed class make a sum mirror (dotty's `isGenericSum`), and their
//! types as the elements of that mirror.

use super::MirrorSynthesis;
use super::child_prefixes::{is_visible_to_scope, owner_scope};
use crate::sealed;
use crate::typer::Typer;
use tasty::tags::{ABSTRACT, CONTRAVARIANT, SEALED};
use tasty::{SymbolId, Type, TypeId, TypeVars, Universe, nme};

impl MirrorSynthesis {
    /// Whether sealed class `s`, selected from prefix `pre`, gets a sum mirror here (dotty's
    /// `isGenericSum`): it has children, none anonymous, each an object or a class with a mirror,
    /// and each visible from here or through `pre` unless its companion serves as the mirror
    /// (`companion`).
    pub fn is_generic_sum(
        self,
        t: &mut Typer<'_>,
        s: SymbolId,
        companion: bool,
        pre: Option<TypeId>,
    ) -> Result<bool, &'static str> {
        let flags = t.u.symbol(s).flags;
        if !flags.has(SEALED) || !(flags.has(ABSTRACT) || t.u.is_trait(s)) {
            return Ok(false);
        }
        let children = t.sealed_children(s)?;
        if children.is_empty() {
            return Ok(false);
        }
        let scope = owner_scope(t.u, pre);
        for c in children {
            if t.u.symbol(c).name.starts_with(nme::ANON_CLASS) {
                return Ok(false);
            }
            let owner = t.u.symbol(c).owner.ok_or("sealed child without owner")?;
            if !sealed::visible_to_parent(t.u, s, owner)
                || !companion && !is_visible_to_scope(t.u, owner, self.ctx.owner, &scope)
            {
                return Ok(false);
            }
            if !t.u.is_class(c) {
                continue;
            }
            let fits = self.is_generic_product(t, c)
                || t.u.symbol(c).flags.has(SEALED) && self.is_generic_sum(t, c, companion, pre)?;
            if !fits {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Determines the element type of child `c` in the mirror of the sum `target` (dotty's `solve`),
/// selected from `prefix` if given: an object's singleton type, or the class with type arguments
/// that make it fit `target`, e.g. `Some[Int]` for `Option[Int]`. Refuses a child whose type
/// arguments `target` does not determine.
pub fn child_type(
    t: &mut Typer<'_>,
    c: SymbolId,
    target: TypeId,
    prefix: Option<TypeId>,
) -> Result<TypeId, &'static str> {
    let raw = match prefix {
        Some(p) => {
            let s = match t.u.is_module_class(c) {
                true => t.u.source_module(c).unwrap_or(c),
                false => c,
            };
            let p = (!matches!(t.u.ty(p), Type::NoPrefix)).then_some(p);
            t.u.intern_type(Type::Sym(p, s))
        }
        None => reachable_ref(t.u, c),
    };
    let params = match t.u.is_class(c) {
        true => t.u.type_params(c),
        false => Vec::new(),
    };
    if params.is_empty() {
        return Ok(raw);
    }
    let mark = t.constraint.mark();
    let vars: Vec<TypeId> = params.iter().map(|_| t.constraint.fresh(t.u)).collect();
    let bounded = bound_child_vars(t, &params, &vars);
    let applied = t.u.intern_applied(raw, &vars);
    let before = t.constraint.mark();
    if !t.conforms(applied, target) {
        t.constraint.rollback(before);
    }
    let solved = bounded && instantiate_child_vars(t, &params, &vars).is_ok();
    let tpe = t.zonk(applied);
    t.constraint.rollback(mark);
    match solved && t.is_fully_defined(tpe) {
        true => Ok(tpe),
        false => Err("sum Mirror of a child whose type parameters are not solved"),
    }
}

/// A reference to `s` reachable from outside its owners (dotty's `reachableTermRef`): through the
/// paths of the objects that are not static it is nested in.
fn reachable_ref(u: &mut Universe, s: SymbolId) -> TypeId {
    let owner = u.symbol(s).owner;
    match owner.filter(|&o| u.is_module_class(o) && !u.is_static_symbol(o)) {
        Some(o) => match u.source_module(o) {
            Some(module) => {
                let prefix = reachable_ref(u, module);
                u.intern_type(Type::Sym(Some(prefix), s))
            }
            None => crate::defn::type_ref(u, s),
        },
        None => crate::defn::type_ref(u, s),
    }
}

/// Constrains `vars` by the declared bounds of type parameters `params` (dotty's `constrained` of
/// the primary constructor's type); `false` if they contradict.
fn bound_child_vars(t: &mut Typer<'_>, params: &[SymbolId], vars: &[TypeId]) -> bool {
    let mut ok = true;
    for (&p, &v) in params.iter().zip(vars) {
        t.complete(p);
        let info = t.u.info(p);
        let Some((lo, hi)) = t.u.bounds(info) else { continue };
        let subst = |u: &mut Universe, x: TypeId| {
            u.map_type(x, &mut |u, x| {
                u.sym_of(x).and_then(|s| params.iter().position(|&q| q == s).map(|i| vars[i]))
            })
        };
        let (lo, hi) = (subst(t.u, lo), subst(t.u, hi));
        t.constraint.declare(t.u, v, lo, hi);
        ok &= t.conforms(lo, v) && t.conforms(v, hi);
    }
    ok
}

fn instantiate_child_vars(
    t: &mut Typer<'_>,
    params: &[SymbolId],
    vars: &[TypeId],
) -> Result<(), &'static str> {
    for (&p, &v) in params.iter().zip(vars) {
        let Some(v) = t.u.var(v) else { continue };
        if t.constraint.instance(v).is_none() {
            let below = t.u.symbol(p).flags.has(CONTRAVARIANT);
            t.instantiate_var(v, below)?;
        }
    }
    Ok(())
}
