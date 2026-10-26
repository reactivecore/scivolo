// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! What a mirrored type reduces to (dotty's `MirrorSource.reduce`): a class with its prefix, a
//! singleton, or the elements of a generic tuple.

use super::super::Typer;
use tasty::tags::{CASE, ENUM};
use tasty::{SymbolId, Type, TypeId};

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

impl Typer<'_> {
    /// The source of the mirror of `t`; `None` where dotty reports that `t` is not mirrored.
    pub fn mirror_source(&mut self, t: TypeId) -> Result<Option<MirrorSource>, &'static str> {
        self.reduced_source(t, MAX_STEPS)
    }

    fn reduced_source(&mut self, t: TypeId, depth: u32) -> Result<Option<MirrorSource>, &'static str> {
        let Some(depth) = depth.checked_sub(1) else { return Err("Mirror of a type too deep to reduce") };
        let next = match self.u.types().get(t).clone() {
            Type::TypeRef(p, n) if self.referenced(t).is_none() => {
                match self.u.refinement_named(p, &n).map(|i| self.u.types().get(i).clone()) {
                    Some(Type::Alias(a) | Type::Bounds(_, a)) => Some(a),
                    _ => return Err("Mirror of an unresolved type"),
                }
            }
            Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => {
                let Some(s) = self.referenced(t) else { return Err("Mirror of an unresolved type") };
                let sym = self.u.symbol(s);
                let (kind, flags) = (sym.kind, sym.flags);
                if kind == tasty::Kind::Class {
                    if !self.u.is_module_class(s) || !flags.has(CASE) {
                        return Ok(Some(MirrorSource::Class(prefix_of(self.u, t), s)));
                    }
                    let module = self.u.source_module(s).ok_or("Mirror of an object without its value")?;
                    let path = self.u.intern_type(Type::Sym(prefix_of(self.u, t), module));
                    return Ok(Some(MirrorSource::Singleton(module, path)));
                }
                if kind.is_term() {
                    if flags.has(ENUM) && flags.has(CASE) {
                        return Ok(Some(MirrorSource::Singleton(s, t)));
                    }
                    self.u.widen_once(t)
                } else {
                    self.upper_type(t)
                }
            }
            Type::Applied(tycon, _) => {
                if let Some(elems) = self.pair_elements(t, tycon)? {
                    return Ok(Some(MirrorSource::Tuple(elems)));
                }
                if self.is_named_tuple(tycon) {
                    let (names, values) = self.named_tuple_parts(t).ok_or("Mirror of a named tuple")?;
                    return Ok(Some(MirrorSource::NamedTuple(names, values)));
                }
                match self.u.dealias(t) {
                    d if d != t => Some(d),
                    _ => Some(tycon),
                }
            }
            Type::Lambda { result, .. } => match self.u.types().get(result) {
                Type::Lambda { .. } => return Ok(None),
                _ => Some(result),
            },
            Type::This(_) | Type::Skolem(..) | Type::Constant(..) | Type::Str(_) => self.u.widen_once(t),
            Type::Annotated(p, _) | Type::Refined(p, ..) | Type::Rec(_, p) => Some(p),
            Type::Match { .. } => match self.u.try_normalize(t) {
                Some(n) => Some(n),
                None => return Ok(None),
            },
            Type::And(l, r) => return self.and_source(l, r, depth),
            Type::Or(..) => return Ok(None),
            Type::Var(_) | Type::Wildcard(_) | Type::Other(_) => {
                return Err("Mirror of a type outside the subset");
            }
            _ => return Ok(None),
        };
        match next {
            Some(n) if n != t => self.reduced_source(n, depth),
            _ => Ok(None),
        }
    }

    /// The more specific source of the two sides of `l & r` (dotty's `isSub`); `None` if they
    /// are unrelated.
    fn and_source(&mut self, l: TypeId, r: TypeId, depth: u32) -> Result<Option<MirrorSource>, &'static str> {
        let (Some(ls), Some(rs)) = (self.reduced_source(l, depth)?, self.reduced_source(r, depth)?) else {
            return Ok(None);
        };
        Ok(if self.is_sub_source(&ls, &rs) {
            Some(ls)
        } else if self.is_sub_source(&rs, &ls) {
            Some(rs)
        } else {
            None
        })
    }

    fn is_sub_source(&mut self, a: &MirrorSource, b: &MirrorSource) -> bool {
        use MirrorSource as M;
        match (a, b) {
            (M::Singleton(s, _), M::Class(_, c)) => {
                let info = self.u.info(*s);
                self.u.class_symbol(info).is_some_and(|k| self.u.derives_from(k, *c))
            }
            (M::Class(_, c1), M::Class(_, c2)) => self.u.derives_from(*c1, *c2),
            (M::Singleton(s1, _), M::Singleton(s2, _)) => s1 == s2,
            (M::Tuple(e1), M::Tuple(e2)) => e1.len() == e2.len() && !e1.is_empty(),
            _ => false,
        }
    }

    /// The upper bound of abstract type or type parameter reference `t`, or its alias.
    fn upper_type(&mut self, t: TypeId) -> Option<TypeId> {
        let d = self.u.dealias(t);
        if d != t {
            return Some(d);
        }
        let s = self.referenced(t)?;
        let prefix = prefix_of(self.u, t);
        let info = match prefix {
            Some(p) => self.u.member_info(p, s),
            None => self.u.info(s),
        };
        match self.u.types().get(info).clone() {
            Type::Bounds(_, hi) => Some(hi),
            _ => None,
        }
    }

    /// The elements of `t` if it is an application of `*:` (`tycon`); refused for one that is no
    /// generic tuple of at most 22 elements.
    fn pair_elements(&mut self, t: TypeId, tycon: TypeId) -> Result<Option<Vec<TypeId>>, &'static str> {
        let pair = self.resolve_full_name("scala.*:", true);
        if pair.is_none() || self.u.class_symbol(tycon) != pair {
            return Ok(None);
        }
        match self.tuple_elements(t).filter(|e| !e.is_empty()) {
            Some(elems) => Ok(Some(elems)),
            None => Err("Mirror of a generic tuple outside the subset"),
        }
    }

    fn is_named_tuple(&mut self, tycon: TypeId) -> bool {
        self.referenced(tycon).is_some_and(|s| self.u.full_name(s) == "scala.NamedTuple$.NamedTuple")
    }
}

/// The prefix of reference `t`.
fn prefix_of(u: &tasty::Universe, t: TypeId) -> Option<TypeId> {
    match *u.types().get(t) {
        Type::Sym(p, _) => p,
        Type::TypeRef(p, _) | Type::TermRef(p, _) => Some(p),
        _ => None,
    }
}
