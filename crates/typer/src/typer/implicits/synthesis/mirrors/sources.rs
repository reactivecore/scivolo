// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! What a mirrored type reduces to (dotty's `MirrorSource.reduce`): a class with its prefix, a
//! singleton, or the elements of a generic tuple.

use crate::typer::Typer;
use crate::typer::type_ops::TypeOps;
use tasty::tags::{CASE, ENUM};
use tasty::{SymbolId, Type, TypeId, Universe};

/// How many steps reducing a mirrored type may take.
const MAX_STEPS: u32 = 32;

#[derive(Clone, PartialEq)]
pub enum MirrorSource {
    /// A class and the prefix of its reference.
    Class(Option<TypeId>, SymbolId),
    /// A case object or enum value case, and the path to it.
    Singleton(SymbolId, TypeId),
    /// The elements of a `*:` chain.
    Tuple(Vec<TypeId>),
    /// The names and value types of a named tuple.
    NamedTuple(Vec<String>, Vec<TypeId>),
}

impl MirrorSource {
    /// Finds what mirrored type `tpe` stands for: a class, a case object or enum case, or a
    /// (named) tuple, looking through aliases, upper bounds, refinements and intersections.
    /// `None` for a type without a mirror, such as a union; refuses a type still to be inferred.
    pub fn of(t: &mut Typer<'_>, tpe: TypeId) -> Result<Option<MirrorSource>, &'static str> {
        reduced_source(t, tpe, MAX_STEPS)
    }

    /// Whether this source is at least as specific as `other`, as `A` is in `A & B` for an `A`
    /// extending `B` (dotty's `isSub`).
    fn is_sub(&self, u: &mut Universe, other: &MirrorSource) -> bool {
        use MirrorSource as M;
        match (self, other) {
            (M::Singleton(s, _), M::Class(_, c)) => {
                let info = u.info(*s);
                u.class_symbol(info).is_some_and(|k| u.derives_from(k, *c))
            }
            (M::Class(_, c1), M::Class(_, c2)) => u.derives_from(*c1, *c2),
            (M::Singleton(s1, _), M::Singleton(s2, _)) => s1 == s2,
            (M::Tuple(e1), M::Tuple(e2)) => e1.len() == e2.len() && !e1.is_empty(),
            _ => false,
        }
    }
}

fn reduced_source(t: &mut Typer<'_>, tpe: TypeId, depth: u32) -> Result<Option<MirrorSource>, &'static str> {
    let Some(depth) = depth.checked_sub(1) else { return Err("Mirror of a type too deep to reduce") };
    let node = *t.u.ty(tpe);
    let next = match node {
        Type::TypeRef(p, n) if t.referenced(tpe).is_none() => {
            match t.u.refinement_named(p, n).map(|i| *t.u.ty(i)) {
                Some(Type::Alias(a) | Type::Bounds(_, a)) => Some(a),
                _ => return Err("Mirror of an unresolved type"),
            }
        }
        Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => {
            let Some(s) = t.referenced(tpe) else { return Err("Mirror of an unresolved type") };
            let sym = t.u.symbol(s);
            let (kind, flags) = (sym.kind, sym.flags);
            if kind == tasty::Kind::Class {
                if !t.u.is_module_class(s) || !flags.has(CASE) {
                    return Ok(Some(MirrorSource::Class(prefix_of(t.u, tpe), s)));
                }
                let module = t.u.source_module(s).ok_or("Mirror of an object without its value")?;
                let path = t.u.intern_type(Type::Sym(prefix_of(t.u, tpe), module));
                return Ok(Some(MirrorSource::Singleton(module, path)));
            }
            if kind.is_term() {
                if flags.has(ENUM) && flags.has(CASE) {
                    return Ok(Some(MirrorSource::Singleton(s, tpe)));
                }
                t.u.widen_once(tpe)
            } else {
                upper_type(&mut t.types(), tpe)
            }
        }
        Type::Applied(tycon, _) => {
            if let Some(elems) = pair_elements(t, tpe, tycon)? {
                return Ok(Some(MirrorSource::Tuple(elems)));
            }
            if is_named_tuple(&mut t.types(), tycon) {
                let (names, values) = t.named_tuple_parts(tpe).ok_or("Mirror of a named tuple")?;
                return Ok(Some(MirrorSource::NamedTuple(names, values)));
            }
            match t.u.dealias(tpe) {
                d if d != tpe => Some(d),
                _ => Some(tycon),
            }
        }
        Type::Lambda { result, .. } => match t.u.lambda(result) {
            Some(_) => return Ok(None),
            None => Some(result),
        },
        Type::This(_) | Type::Skolem(..) | Type::Constant(..) | Type::Str(_) => t.u.widen_once(tpe),
        Type::Annotated(p, _) | Type::Refined(p, ..) | Type::Rec(_, p) => Some(p),
        Type::Match { .. } => match t.u.try_normalize(tpe) {
            Some(n) => Some(n),
            None => return Ok(None),
        },
        Type::And(l, r) => return and_source(t, l, r, depth),
        Type::Or(..) => return Ok(None),
        Type::Var(_) | Type::Wildcard(_) | Type::Other(_) => {
            return Err("Mirror of a type outside the subset");
        }
        _ => return Ok(None),
    };
    match next {
        Some(n) if n != tpe => reduced_source(t, n, depth),
        _ => Ok(None),
    }
}

/// The more specific source of the two sides of `l & r` (dotty's `isSub`); `None` if they are
/// unrelated.
fn and_source(
    t: &mut Typer<'_>,
    l: TypeId,
    r: TypeId,
    depth: u32,
) -> Result<Option<MirrorSource>, &'static str> {
    let (Some(ls), Some(rs)) = (reduced_source(t, l, depth)?, reduced_source(t, r, depth)?) else {
        return Ok(None);
    };
    Ok(if ls.is_sub(t.u, &rs) {
        Some(ls)
    } else if rs.is_sub(t.u, &ls) {
        Some(rs)
    } else {
        None
    })
}

/// The upper bound of abstract type or type parameter reference `t`, or its alias.
fn upper_type(ops: &mut TypeOps<'_>, t: TypeId) -> Option<TypeId> {
    let d = ops.u.dealias(t);
    if d != t {
        return Some(d);
    }
    let s = ops.referenced(t)?;
    let prefix = prefix_of(ops.u, t);
    let info = match prefix {
        Some(p) => ops.u.member_info(p, s),
        None => ops.u.info(s),
    };
    ops.u.bounds(info).map(|(_, hi)| hi)
}

/// The elements of `t` if it is an application of `*:` (`tycon`); refused for one that is no
/// generic tuple of at most 22 elements.
fn pair_elements(t: &mut Typer<'_>, tpe: TypeId, tycon: TypeId) -> Result<Option<Vec<TypeId>>, &'static str> {
    let pair = t.resolve_full_name("scala.*:", true);
    if pair.is_none() || t.u.class_symbol(tycon) != pair {
        return Ok(None);
    }
    match t.tuple_elements(tpe).filter(|e| !e.is_empty()) {
        Some(elems) => Ok(Some(elems)),
        None => Err("Mirror of a generic tuple outside the subset"),
    }
}

fn is_named_tuple(ops: &mut TypeOps<'_>, tycon: TypeId) -> bool {
    ops.referenced(tycon).is_some_and(|s| ops.u.full_name(s) == crate::defn::NAMED_TUPLE)
}

/// The prefix of reference `t`.
fn prefix_of(u: &Universe, t: TypeId) -> Option<TypeId> {
    match *u.ty(t) {
        Type::Sym(p, _) => p,
        Type::TypeRef(p, _) | Type::TermRef(p, _) => Some(p),
        _ => None,
    }
}
