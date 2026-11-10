// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The size of a type and the symbols it is built from, which the divergence check compares
//! (dotty's `typeSize`, `coveringSet`, their `TypeSizeAccumulator` and `CoveringSetAccumulator`).

use super::super::type_ops::TypeOps;
use tasty::{SymbolId, Type, TypeId, TypeVars, Universe, VarId};

const OUTSIDE: &str = "divergence check of a type outside the subset";

/// The parts a covering set folds over, and the symbol it adds.
type Fold = (Vec<TypeId>, Option<SymbolId>);

/// dotty's `typeSize` and `coveringSet`.
#[derive(Clone, PartialEq)]
pub struct Measure {
    pub size: u32,
    pub covering: Vec<SymbolId>,
}

impl Measure {
    pub fn of(ty: &mut TypeOps<'_>, tpe: TypeId) -> Result<Measure, &'static str> {
        let tpe = ty.zonk(tpe);
        let size = type_size(ty, 0, tpe, &mut Vec::new())?;
        let mut covering = Vec::new();
        cover(ty, &mut covering, tpe, &mut Vec::new())?;
        covering.sort_unstable_by_key(|s| s.0);
        covering.dedup();
        Ok(Measure { size, covering })
    }
}

/// The parts of `t` a size or covering set folds over: its prefix, arguments and operands.
fn parts(u: &mut Universe, t: TypeId) -> Result<Vec<TypeId>, &'static str> {
    Ok(match u.types().get(t).clone() {
        Type::Sym(p, _) => p.into_iter().collect(),
        Type::TypeRef(p, _) | Type::TermRef(p, _) | Type::This(p) => vec![p],
        Type::Applied(c, args) => [vec![c], args].concat(),
        Type::Refined(p, _, i) | Type::Bounds(p, i) | Type::And(p, i) | Type::Or(p, i, _) => vec![p, i],
        Type::ByName(r) | Type::Annotated(r, _) | Type::Alias(r) => vec![r],
        Type::Lambda { params, result, .. } => {
            if !params.iter().all(|&(_, b)| unbounded(u, b)) {
                return Err(OUTSIDE);
            }
            params.into_iter().map(|(_, b)| b).chain([result]).collect()
        }
        Type::Param(..)
        | Type::Local(..)
        | Type::NoPrefix
        | Type::Package(_)
        | Type::PackageName(_)
        | Type::Str(_)
        | Type::Constant(..) => Vec::new(),
        _ => return Err(OUTSIDE),
    })
}

/// Whether `b` is the bounds `>: Nothing <: Any`.
fn unbounded(u: &mut Universe, b: TypeId) -> bool {
    let Type::Bounds(lo, hi) = *u.types().get(b) else { return false };
    let builtins = u.builtins();
    u.class_symbol(lo) == Some(builtins.nothing) && u.class_symbol(hi) == Some(builtins.any)
}

/// `t` dealiased if it is a reference to an alias other than a match type alias (dotty's
/// `isTypeAlias`), `None` if not.
fn alias_of(u: &mut Universe, t: TypeId) -> Option<TypeId> {
    if !matches!(u.types().get(t), Type::Sym(..) | Type::TypeRef(..)) {
        return None;
    }
    let d = u.dealias(t);
    let body = match *u.types().get(d) {
        Type::Lambda { result, .. } => result,
        _ => d,
    };
    (d != t && !matches!(u.types().get(body), Type::Match { .. })).then_some(d)
}

/// The bounds of type variable `v`, match types in them reduced.
fn reduced_bounds(ty: &mut TypeOps<'_>, v: VarId) -> [TypeId; 2] {
    let (lo, hi) = ty.constraint.bounds(v);
    [lo, hi].map(|b| {
        let b = ty.zonk(b);
        ty.u.try_normalize(b).unwrap_or(b)
    })
}

fn type_size(ty: &mut TypeOps<'_>, n: u32, tpe: TypeId, seen: &mut Vec<VarId>) -> Result<u32, &'static str> {
    if let Some(d) = alias_of(ty.u, tpe) {
        return type_size(ty, n, d, seen);
    }
    match ty.u.types().get(tpe).clone() {
        Type::Var(v) if seen.contains(&v) => Ok(n),
        Type::Var(v) => {
            seen.push(v);
            let [lo, hi] = reduced_bounds(ty, v);
            Ok(type_size(ty, n, lo, seen)?.max(type_size(ty, n, hi, seen)?))
        }
        Type::Applied(..) if let Some(reduced) = ty.u.try_normalize(tpe).filter(|&r| r != tpe) => {
            type_size(ty, n, reduced, seen)
        }
        Type::Applied(c, args) if tuple_arity(ty.u, c) == Some(args.len()) => {
            let mut n = n + args.len() as u32;
            for a in args {
                n = type_size(ty, n, a, seen)?;
            }
            Ok(n)
        }
        other => {
            let mut n = n + matches!(other, Type::Applied(..) | Type::Refined(..)) as u32;
            for p in parts(ty.u, tpe)? {
                n = type_size(ty, n, p, seen)?;
            }
            Ok(n)
        }
    }
}

/// `N` if `c` is the class `scala.TupleN`.
fn tuple_arity(u: &mut Universe, c: TypeId) -> Option<usize> {
    let class = u.class_symbol(c)?;
    let name = u.full_name(class);
    let max = crate::typer::small_tuples::MAX_TUPLE_ARITY;
    name.strip_prefix(crate::defn::TUPLE)?.parse().ok().filter(|&n| (2..=max).contains(&n))
}

/// Adds the symbols `tpe` is built from to `cs`, those of the prefixes of references to symbols
/// that are not static included (dotty's `StopAt.Static`).
fn cover(
    ty: &mut TypeOps<'_>,
    cs: &mut Vec<SymbolId>,
    tpe: TypeId,
    seen: &mut Vec<TypeId>,
) -> Result<(), &'static str> {
    if seen.contains(&tpe) {
        return Ok(());
    }
    seen.push(tpe);
    if let Some(d) = alias_of(ty.u, tpe) {
        return cover(ty, cs, d, seen);
    }
    let (fold, own) = match *ty.u.types().get(tpe) {
        Type::Var(v) => (reduced_bounds(ty, v).to_vec(), None),
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
    for p in fold {
        cover(ty, cs, p, seen)?;
    }
    Ok(())
}

/// The parts `cover` folds over for `t`, not a type variable, and the symbol it adds; `None` for
/// `Any` and `Nothing`.
fn folded(u: &mut Universe, t: TypeId) -> Result<Option<Fold>, &'static str> {
    let b = u.builtins();
    Ok(Some(match u.types().get(t).clone() {
        Type::Applied(c, args) => ([vec![c], args].concat(), type_symbol(u, c)),
        Type::Refined(p, _, i) => (vec![p, i], type_symbol(u, p)),
        Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => {
            let (p, s) = match *u.types().get(t) {
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
    match *u.types().get(t) {
        Type::Sym(_, s) => Some(s),
        Type::Applied(c, _) => type_symbol(u, c),
        _ => u.class_symbol(t),
    }
}
