// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The size of a type and the symbols it is built from, which the divergence check compares
//! (dotty's `typeSize`, `coveringSet`, their `TypeSizeAccumulator` and `CoveringSetAccumulator`).

use super::super::type_ops::TypeOps;
use super::covering::cover;
use tasty::{Binder, ParamList, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

/// The refusal of a divergence check of a type the measures do not cover.
pub const OUTSIDE: &str = "divergence check of a type outside the subset";

/// dotty's `typeSize` and `coveringSet`.
#[derive(Clone, PartialEq)]
pub struct Measure {
    pub size: u32,
    pub covering: Vec<SymbolId>,
}

impl Measure {
    pub fn of(ty: &mut TypeOps<'_>, tpe: TypeId) -> Result<Measure, &'static str> {
        let tpe = ty.zonk(tpe);
        let size = type_size(ty, 0, tpe, &mut Walk::default())?;
        let mut covering = Vec::new();
        cover(ty, &mut covering, tpe, &mut Walk::default())?;
        covering.sort_unstable_by_key(|s| s.0);
        covering.dedup();
        Ok(Measure { size, covering })
    }
}

/// The type lambdas around the part a walk visits, and the variables, parameters and types it
/// has passed.
#[derive(Default)]
pub struct Walk {
    lambdas: Vec<(Binder, ParamList)>,
    vars: Vec<VarId>,
    params: Vec<(Binder, u32)>,
    pub types: Vec<TypeId>,
}

impl Walk {
    /// Determines the bounds of parameter `i` of the type lambda `binder` around the walked part.
    pub fn param_bounds(&self, u: &Universe, binder: Binder, i: u32) -> Option<TypeId> {
        let &(_, params) = self.lambdas.iter().rev().find(|(b, _)| *b == binder)?;
        u.params_of(params).get(i as usize).map(|&(_, b)| b)
    }

    /// Enters the type lambda `t`, if it is one; `leave` with the result leaves it.
    pub fn enter(&mut self, u: &Universe, t: TypeId) -> bool {
        let Type::Lambda { binder, params, .. } = *u.ty(t) else { return false };
        self.lambdas.push((binder, params));
        true
    }

    /// Leaves the type lambda `enter` entered.
    pub fn leave(&mut self, entered: bool) {
        if entered {
            self.lambdas.pop();
        }
    }
}

/// The parts of `t` a size or covering set folds over: its prefix, arguments and operands, the
/// bounds of a type lambda's parameters and its result.
pub fn parts(u: &mut Universe, t: TypeId) -> Result<Vec<TypeId>, &'static str> {
    Ok(match *u.ty(t) {
        Type::Sym(p, _) => p.into_iter().collect(),
        Type::TypeRef(p, _) | Type::TermRef(p, _) | Type::This(p) => vec![p],
        Type::Applied(c, args) => [&[c][..], u.types_of(args)].concat(),
        Type::Refined(p, _, i) | Type::Bounds(p, i) | Type::And(p, i) | Type::Or(p, i, _) => vec![p, i],
        Type::ByName(r) | Type::Annotated(r, _) | Type::Alias(r) => vec![r],
        Type::Lambda { params, result, .. } => {
            u.params_of(params).iter().map(|&(_, b)| b).chain([result]).collect()
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

/// `t` dealiased if it is a reference to an alias other than a match type alias (dotty's
/// `isTypeAlias`), `None` if not.
pub fn alias_of(u: &mut Universe, t: TypeId) -> Option<TypeId> {
    if !matches!(u.ty(t), Type::Sym(..) | Type::TypeRef(..)) {
        return None;
    }
    let d = u.dealias(t);
    let body = u.lambda(d).map_or(d, |l| l.result);
    (d != t && !matches!(u.ty(body), Type::Match { .. })).then_some(d)
}

/// The bounds of type variable `v`, match types in them reduced.
pub fn reduced_bounds(ty: &mut TypeOps<'_>, v: VarId) -> [TypeId; 2] {
    let (lo, hi) = ty.constraint.bounds(v);
    [lo, hi].map(|b| {
        let b = ty.zonk(b);
        ty.u.try_normalize(b).unwrap_or(b)
    })
}

fn type_size(ty: &mut TypeOps<'_>, n: u32, tpe: TypeId, walk: &mut Walk) -> Result<u32, &'static str> {
    if let Some(d) = alias_of(ty.u, tpe) {
        return type_size(ty, n, d, walk);
    }
    let node = *ty.u.ty(tpe);
    match node {
        Type::Var(v) if walk.vars.contains(&v) => Ok(n),
        Type::Var(v) => {
            walk.vars.push(v);
            let [lo, hi] = reduced_bounds(ty, v);
            Ok(type_size(ty, n, lo, walk)?.max(type_size(ty, n, hi, walk)?))
        }
        Type::Param(b, i)
            if !walk.params.contains(&(b, i))
                && let Some(bounds) = walk.param_bounds(ty.u, b, i) =>
        {
            walk.params.push((b, i));
            let (lo, hi) = ty.u.bounds(bounds).ok_or(OUTSIDE)?;
            Ok(type_size(ty, n, lo, walk)?.max(type_size(ty, n, hi, walk)?))
        }
        Type::Applied(..) if let Some(reduced) = ty.u.try_normalize(tpe).filter(|&r| r != tpe) => {
            type_size(ty, n, reduced, walk)
        }
        Type::Applied(c, args) if tuple_arity(ty.u, c) == Some(ty.u.types_of(args).len()) => {
            let mut n = n + ty.u.types_of(args).len() as u32;
            for i in 0..ty.u.types_of(args).len() {
                let a = ty.u.types_of(args)[i];
                n = type_size(ty, n, a, walk)?;
            }
            Ok(n)
        }
        other => {
            let mut n = n + matches!(other, Type::Applied(..) | Type::Refined(..)) as u32;
            let entered = walk.enter(ty.u, tpe);
            for p in parts(ty.u, tpe)? {
                n = type_size(ty, n, p, walk)?;
            }
            walk.leave(entered);
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
