// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Approximations of mapped types by ranges (dotty's `ApproximatingTypeMap`).

use tasty::{ANY, NOTHING, Type, TypeId, Universe};

/// A mapped type, or the range of types between a lower and an upper bound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Approx {
    Exact(TypeId),
    Range(TypeId, TypeId),
}

impl Approx {
    pub fn lo(self) -> TypeId {
        match self {
            Approx::Exact(t) | Approx::Range(t, _) => t,
        }
    }

    pub fn hi(self) -> TypeId {
        match self {
            Approx::Exact(t) | Approx::Range(_, t) => t,
        }
    }

    pub fn is_range(self) -> bool {
        matches!(self, Approx::Range(..))
    }

    /// The bounds of a range as a wildcard argument (dotty's `rangeToBounds`).
    pub fn to_bounds(self, u: &mut Universe) -> TypeId {
        match self {
            Approx::Exact(t) => t,
            Approx::Range(lo, hi) => {
                let lo = match *u.types().get(lo) {
                    Type::Bounds(l, _) => l,
                    _ => lo,
                };
                let hi = match *u.types().get(hi) {
                    Type::Bounds(_, h) => h,
                    _ => hi,
                };
                u.intern_type(Type::Bounds(lo, hi))
            }
        }
    }
}

/// The types between `lo` and `hi` at variance `v`: the bound `v` chooses, or the range.
pub fn range(u: &mut Universe, lo: Approx, hi: Approx, v: i8) -> Approx {
    let (lo, hi) = (lo.lo(), hi.hi());
    if v > 0 {
        Approx::Exact(hi)
    } else if v < 0 && lo == NOTHING {
        Approx::Exact(u.intern_type(Type::And(NOTHING, hi)))
    } else if v < 0 || lo == hi {
        Approx::Exact(lo)
    } else {
        Approx::Range(lo, hi)
    }
}

/// `make(parts)` for exact parts, the range of `make` over the lower and the upper parts
/// otherwise; `flip[i]` takes part `i`'s bounds the other way round.
pub fn derived(
    u: &mut Universe,
    parts: &[Approx],
    flip: &[bool],
    v: i8,
    make: &mut dyn FnMut(&mut Universe, &[TypeId]) -> TypeId,
) -> Approx {
    let pick = |upper: bool| -> Vec<TypeId> {
        let flipped = |i: usize| flip.get(i).copied().unwrap_or(false);
        parts.iter().enumerate().map(|(i, p)| if upper != flipped(i) { p.hi() } else { p.lo() }).collect()
    };
    if !parts.iter().any(|p| p.is_range()) {
        return Approx::Exact(make(u, &pick(true)));
    }
    let lo = make(u, &pick(false));
    let hi = make(u, &pick(true));
    range(u, Approx::Exact(lo), Approx::Exact(hi), v)
}

/// `tycon[args]` (dotty's `derivedAppliedType`): range arguments become wildcards at a
/// positive variance, are distributed over variant parameters otherwise, and give
/// `Nothing .. tycon[wildcards]` at an invariant one.
pub fn applied(u: &mut Universe, tycon: Approx, args: &[Approx], variances: &[i8], v: i8) -> Approx {
    if let Approx::Range(lo, hi) = tycon {
        let lo = applied(u, Approx::Exact(lo), args, variances, v);
        let hi = applied(u, Approx::Exact(hi), args, variances, v);
        return range(u, lo, hi, v);
    }
    let tycon = tycon.hi();
    if !args.iter().any(|a| a.is_range()) {
        let args = args.iter().map(|a| a.hi()).collect();
        return Approx::Exact(u.intern_type(Type::Applied(tycon, args)));
    }
    let wild = |u: &mut Universe| {
        let wild: Vec<TypeId> = args.iter().map(|a| a.to_bounds(u)).collect();
        u.intern_type(Type::Applied(tycon, wild))
    };
    if v > 0 {
        return Approx::Exact(wild(u));
    }
    let variance = |i: usize| variances.get(i).copied().unwrap_or(0);
    if args.iter().enumerate().all(|(i, a)| !a.is_range() || variance(i) != 0) {
        let flip: Vec<bool> = (0..args.len()).map(|i| variance(i) < 0).collect();
        return derived(u, args, &flip, v, &mut |u, args| u.intern_type(Type::Applied(tycon, args.to_vec())));
    }
    let hi = match args.iter().any(|&a| is_range_of_bounds(u, a)) {
        true => ANY,
        false => wild(u),
    };
    range(u, Approx::Exact(NOTHING), Approx::Exact(hi), v)
}

/// The variances of the `n` parameters of type constructor `tycon`, invariant where unknown.
pub fn tycon_variances(u: &mut Universe, tycon: TypeId, n: usize) -> Vec<i8> {
    let s = match *u.types().get(tycon) {
        Type::Sym(_, s) => Some(s),
        Type::TypeRef(..) => u.resolve_ref(tycon),
        _ => None,
    };
    s.map(|s| u.variances(s)).filter(|vs| vs.len() == n).unwrap_or_else(|| vec![0; n])
}

/// Whether `a` is a range with bounds or an alias as one of its ends (dotty's
/// `isRangeOfNonTermTypes`).
fn is_range_of_bounds(u: &Universe, a: Approx) -> bool {
    let bounds = |t| matches!(u.types().get(t), Type::Bounds(..) | Type::Alias(_) | Type::Wildcard(_));
    a.is_range() && (bounds(a.lo()) || bounds(a.hi()))
}

/// Type bounds `lo .. hi` (dotty's `derivedTypeBounds`).
pub fn bounds(u: &mut Universe, lo: Approx, hi: Approx, v: i8) -> Approx {
    if !lo.is_range() && !hi.is_range() {
        return Approx::Exact(u.intern_type(Type::Bounds(lo.hi(), hi.hi())));
    }
    let wide = u.intern_type(Type::Bounds(lo.lo(), hi.hi()));
    if v > 0 {
        return Approx::Exact(wide);
    }
    let narrow = u.intern_type(Type::Bounds(lo.hi(), hi.lo()));
    range(u, Approx::Exact(narrow), Approx::Exact(wide), v)
}

/// Type alias `= a` (dotty's `derivedAlias`).
pub fn alias(u: &mut Universe, a: Approx, v: i8) -> Approx {
    match a {
        Approx::Exact(a) => Approx::Exact(u.intern_type(Type::Alias(a))),
        Approx::Range(lo, hi) if v > 0 => Approx::Exact(u.intern_type(Type::Bounds(lo, hi))),
        Approx::Range(lo, hi) => {
            let (lo, hi) = (u.intern_type(Type::Alias(lo)), u.intern_type(Type::Alias(hi)));
            range(u, Approx::Exact(lo), Approx::Exact(hi), v)
        }
    }
}

/// `parent { name: info }` (dotty's `derivedRefinedType`).
pub fn refined(u: &mut Universe, parent: Approx, name: &str, info: Approx, v: i8) -> Approx {
    if let Approx::Range(lo, hi) = parent {
        let lo = refined(u, Approx::Exact(lo), name, info, v);
        let hi = refined(u, Approx::Exact(hi), name, info, v);
        return range(u, lo, hi, v);
    }
    let parent = parent.hi();
    if parent == NOTHING {
        return Approx::Exact(parent);
    }
    let Approx::Range(lo, hi) = info else {
        return Approx::Exact(u.intern_type(Type::Refined(parent, name.to_owned(), info.hi())));
    };
    let is_alias = |u: &Universe, t| matches!(u.types().get(t), Type::Alias(_));
    let is_bounds = |u: &Universe, t| matches!(u.types().get(t), Type::Alias(_) | Type::Bounds(..));
    if is_bounds(u, lo) && is_bounds(u, hi) && (is_alias(u, lo) || is_alias(u, hi)) {
        return range(u, Approx::Exact(NOTHING), Approx::Exact(parent), v);
    }
    let lo = u.intern_type(Type::Refined(parent, name.to_owned(), lo));
    let hi = u.intern_type(Type::Refined(parent, name.to_owned(), hi));
    range(u, Approx::Exact(lo), Approx::Exact(hi), v)
}
