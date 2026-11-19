// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Type member extractors `C { type T = x }` in match type patterns (dotty's
//! `MatchTypeCasePattern.TypeMemberExtractor`).

use super::super::Cmp;
use super::spec::Pattern;
use crate::types::{Binder, NameId, Type, TypeId};
use crate::{Kind, SymbolId, Universe};

/// How many aliases on a skolem `drop_skolem` follows.
const MAX_ALIASES: u32 = 16;

/// The captures of a match type case: their binder, names and bounds.
pub struct Captures<'a> {
    pub binder: Binder,
    pub names: &'a [NameId],
    pub bounds: &'a [TypeId],
}

impl Cmp<'_> {
    /// The pattern of `parent { name info }`: a type test unless `info` aliases a capture, else
    /// the extractor of the member `name` of `parent` if the capture's bounds contain the
    /// member's; `None` for a member `parent` lacks or one with wider bounds.
    pub fn refined_pattern(
        &mut self,
        (parent, name, info): (TypeId, NameId, TypeId),
        captures: &Captures,
        unaccounted: &mut [bool],
    ) -> Option<Pattern> {
        let Type::Alias(alias) = *self.u.ty(info) else { return Some(Pattern::TypeTest) };
        let Type::Param(b, i) = *self.u.ty(alias) else { return Some(Pattern::TypeTest) };
        if b != captures.binder {
            return Some(Pattern::TypeTest);
        }
        let member =
            self.u.member_of_type(parent, name).into_iter().find(|&m| self.u.symbol(m).kind == Kind::Type)?;
        let info = self.u.member_info(parent, member);
        let (lo, hi) = match alias_of(self.u, info) {
            Some(a) => (a, a),
            None => self.u.bounds(info)?,
        };
        let (capture_lo, capture_hi) = self.u.bounds(*captures.bounds.get(i as usize)?)?;
        if !self.u.is_subtype(capture_lo, lo) || !self.u.is_subtype(hi, capture_hi) {
            return None;
        }
        let capture = self.spec_pattern(alias, 0, captures, unaccounted)?;
        Some(Pattern::TypeMemberExtractor { name, capture: Box::new(capture) })
    }

    /// The type member `name` of `scrut`: selected on `scrut` if a singleton, else on a skolem of
    /// it, its alias without the skolem, or bounds that make the capture not specific.
    pub fn extracted_member(&mut self, scrut: TypeId, name: NameId) -> Option<TypeId> {
        let members = self.u.member_of_type(scrut, name).into_iter().filter(|&m| self.u.is_type_symbol(m));
        let members: Vec<SymbolId> = members.collect();
        let &first = members.first()?;
        if self.is_singleton(scrut) {
            return Some(self.u.intern_type(Type::Sym(Some(scrut), first)));
        }
        let skolem = self.u.fresh_skolem(scrut);
        let infos: Vec<TypeId> = members.into_iter().map(|m| self.u.member_info(skolem, m)).collect();
        let not_specific = |u: &mut Universe, t: TypeId| match u.bounds(t) {
            Some(_) => t,
            None => u.intern_type(Type::Bounds(t, t)),
        };
        let Some(alias) = infos.iter().find_map(|&info| alias_of(self.u, info)) else {
            return Some(not_specific(self.u, infos[0]));
        };
        let mut refers = false;
        let dropped = drop_skolem(self.u, alias, skolem, &mut refers, MAX_ALIASES);
        if refers {
            return Some(not_specific(self.u, alias));
        }
        let reduced = self.u.map_type(dropped, &mut |u, x| Some(u.beta_reduced(x)).filter(|&r| r != x));
        Some(self.normal(reduced))
    }
}

/// The alias of type member info `info`: an alias, or a type lambda.
fn alias_of(u: &mut Universe, info: TypeId) -> Option<TypeId> {
    match *u.ty(info) {
        Type::Lambda { .. } => Some(info),
        _ => u.type_alias(info),
    }
}

/// `t` with each type member of `skolem` replaced by its alias (dotty's `DropSkolemMap`);
/// `refers` set if a reference to `skolem` remains.
fn drop_skolem(u: &mut Universe, t: TypeId, skolem: TypeId, refers: &mut bool, depth: u32) -> TypeId {
    u.map_type(t, &mut |u, x| {
        if *refers {
            return Some(x);
        }
        if x == skolem {
            *refers = true;
            return Some(x);
        }
        let (p, name) = match *u.ty(x) {
            Type::Sym(Some(p), s) => (p, u.interned_name(s)),
            Type::TypeRef(p, name) | Type::TermRef(p, name) => (p, name),
            _ => return None,
        };
        if p != skolem {
            return None;
        }
        let members = u.member_of_type(skolem, name).into_iter().filter(|&m| u.symbol(m).kind == Kind::Type);
        let members: Vec<_> = members.collect();
        let alias = members.into_iter().find_map(|m| {
            let info = u.member_info(skolem, m);
            alias_of(u, info)
        });
        match alias.zip(depth.checked_sub(1)) {
            Some((a, depth)) => Some(drop_skolem(u, a, skolem, refers, depth)),
            None => {
                *refers = true;
                Some(x)
            }
        }
    })
}
