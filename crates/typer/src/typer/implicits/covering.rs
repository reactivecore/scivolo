// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The symbols a type is built from, which the divergence check compares (dotty's
//! `coveringSet` and its `CoveringSetAccumulator`).

use super::super::type_ops::TypeOps;
use super::measures::{OUTSIDE, Walk, alias_of, parts, reduced_bounds};
use tasty::{SymbolId, Type, TypeId, Universe};

/// The parts a covering set folds over, and the symbol it adds.
type Fold = (Vec<TypeId>, Option<SymbolId>);

/// Adds the symbols `tpe` is built from to `cs`, those of the prefixes of references to symbols
/// that are not static included (dotty's `StopAt.Static`), and those of the bounds of the type
/// lambda parameters `tpe` refers to.
pub fn cover(
    ty: &mut TypeOps<'_>,
    cs: &mut Vec<SymbolId>,
    tpe: TypeId,
    walk: &mut Walk,
) -> Result<(), &'static str> {
    if walk.types.contains(&tpe) {
        return Ok(());
    }
    walk.types.push(tpe);
    if let Some(d) = alias_of(ty.u, tpe) {
        return cover(ty, cs, d, walk);
    }
    let (fold, own) = match *ty.u.ty(tpe) {
        Type::Var(v) => (reduced_bounds(ty, v).to_vec(), None),
        Type::Param(b, i) if let Some(bounds) = walk.param_bounds(ty.u, b, i) => (vec![bounds], None),
        _ => match folded(ty.u, tpe)? {
            Some(folded) => folded,
            None => return Ok(()),
        },
    };
    if let Some(s) = own {
        if ty.u.symbol(s).kind == tasty::Kind::Param {
            return Err(OUTSIDE);
        }
        cs.push(s);
    }
    let entered = walk.enter(ty.u, tpe);
    for p in fold {
        cover(ty, cs, p, walk)?;
    }
    walk.leave(entered);
    Ok(())
}

/// The parts `cover` folds over for `t`, not a type variable, and the symbol it adds; `None` for
/// `Any` and `Nothing`.
fn folded(u: &mut Universe, t: TypeId) -> Result<Option<Fold>, &'static str> {
    let b = u.defn();
    Ok(Some(match *u.ty(t) {
        Type::Applied(c, args) => ([&[c][..], u.types_of(args)].concat(), type_symbol(u, c)),
        Type::Refined(p, _, i) => (vec![p, i], type_symbol(u, p)),
        Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => {
            let (p, s) = match *u.ty(t) {
                Type::Sym(p, s) => (p, s),
                Type::TypeRef(p, _) | Type::TermRef(p, _) => (Some(p), u.resolve_ref(t).ok_or(OUTSIDE)?),
                _ => return Err(OUTSIDE),
            };
            if s == b.any || s == b.nothing {
                return Ok(None);
            }
            let own = (u.symbol(s).is_class() || !u.is_type_symbol(s)).then_some(s);
            (p.filter(|_| !u.is_static_symbol(s)).into_iter().collect(), own)
        }
        Type::This(c) => (Vec::new(), type_symbol(u, c)),
        Type::Package(p) => (Vec::new(), Some(p)),
        _ => (parts(u, t)?, None),
    }))
}

/// The symbol a reference or applied type refers to.
fn type_symbol(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    match *u.ty(t) {
        Type::Sym(_, s) => Some(s),
        Type::Applied(c, _) => type_symbol(u, c),
        _ => u.class_symbol(t),
    }
}
