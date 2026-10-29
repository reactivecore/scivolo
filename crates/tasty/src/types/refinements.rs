// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The type members refinements give a path's type (dotty's `goRefined`, `lookupRefined`), and
//! types without their refinements.

use super::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

/// How many steps finding a refinement takes at most.
const MAX_STEPS: u32 = 64;

impl Universe {
    /// The info a refinement of `prefix`'s widened type gives type member `s`.
    pub fn refinement_info(&mut self, prefix: TypeId, s: SymbolId) -> Option<TypeId> {
        if self.symbol(s).kind != Kind::Type {
            return None;
        }
        let name = self.symbol(s).name.clone();
        self.refinement_named(prefix, &name)
    }

    /// The info a refinement of `prefix`'s widened type gives type member `name`, a member
    /// that need not have a symbol: its bounds or alias.
    pub fn refinement_named(&mut self, prefix: TypeId, name: &str) -> Option<TypeId> {
        let mut t = self.widen(prefix);
        for _ in 0..MAX_STEPS {
            match self.types().get(t).clone() {
                Type::Refined(_, n, info)
                    if n == name && matches!(self.types().get(info), Type::Alias(_) | Type::Bounds(..)) =>
                {
                    return Some(info);
                }
                Type::Refined(p, ..) | Type::Annotated(p, _) => t = p,
                Type::And(a, b) => {
                    let (a, b) = (self.refinement_named(a, name), self.refinement_named(b, name));
                    let (Some(a), Some(b)) = (a, b) else { return a.or(b) };
                    return match (self.types().get(a).clone(), self.types().get(b).clone()) {
                        (Type::Bounds(lx, hx), Type::Bounds(ly, hy)) => {
                            let (lo, hi) = (self.lub(lx, ly), self.glb(hx, hy));
                            Some(self.intern_type(Type::Bounds(lo, hi)))
                        }
                        (Type::Alias(_), _) => Some(a),
                        _ => Some(b),
                    };
                }
                _ => match self.dealias(t) {
                    d if d != t => t = d,
                    _ => return None,
                },
            }
        }
        None
    }

    /// The alias or class by which the class of term `prefix`'s type as seen from its own prefix,
    /// or of `prefix` a `this`, overrides abstract type member `s`, seen from `prefix`.
    pub fn overriding_alias(&mut self, prefix: TypeId, s: SymbolId) -> Option<TypeId> {
        if self.symbol(s).kind != Kind::Type {
            return None;
        }
        let declared = match *self.types().get(prefix) {
            Type::Sym(p, term) if self.symbol(term).kind.is_term() => self.term_type(p, term),
            Type::TermRef(p, _) => {
                let term = self.resolve_ref(prefix)?;
                self.term_type(Some(p), term)
            }
            Type::This(c) => c,
            _ => return None,
        };
        let base = self.symbol(s).owner?;
        let cls = self.class_symbol(declared).filter(|&c| c != base && self.derives_from(c, base))?;
        let name = self.symbol(s).name.clone();
        let found = self
            .member_where(cls, &name, |u, m| m != s && matches!(u.symbol(m).kind, Kind::Type | Kind::Class));
        let (&m, owner) = (found.first()?, self.symbol(found[0]).owner?);
        if self.symbol(m).kind == Kind::Class {
            return Some(self.intern_type(Type::Sym(Some(prefix), m)));
        }
        let alias = self.alias_of(m)?;
        Some(self.as_seen_from(alias, prefix, owner))
    }

    /// `t` without its refinements, and whether one of them refines a term member.
    pub fn without_refinements(&mut self, mut t: TypeId) -> (TypeId, bool) {
        let mut term = false;
        while let Type::Refined(p, _, info) = *self.types().get(t) {
            term |= !matches!(self.types().get(info), Type::Alias(_) | Type::Bounds(..));
            t = p;
        }
        (t, term)
    }

    /// `t` with its references to type members that refinements of `refined` alias replaced by
    /// their aliases.
    pub fn with_refined_aliases(&mut self, t: TypeId, refined: TypeId) -> TypeId {
        if !matches!(self.types().get(refined), Type::Refined(..)) {
            return t;
        }
        self.map_type(t, &mut |u, part| {
            let Type::Sym(Some(_), s) = *u.types().get(part) else { return None };
            let info = u.refinement_info(refined, s)?;
            match *u.types().get(info) {
                Type::Alias(a) => Some(a),
                _ => None,
            }
        })
    }

    /// The method type a refinement of `prefix`'s widened type gives `apply` method `s` of a
    /// function class if its result refers to its parameters (a dependent function type).
    pub fn dependent_refinement(&mut self, prefix: TypeId, s: SymbolId) -> Option<TypeId> {
        let sym = self.symbol(s);
        let function = |n: &str| n.starts_with("Function") || n.starts_with("ContextFunction");
        if sym.kind != Kind::Def
            || sym.name != "apply"
            || !sym.owner.is_some_and(|o| function(&self.symbol(o).name))
        {
            return None;
        }
        let info = self.term_refinement_named(prefix, "apply")?;
        let Type::Method { binder, result, .. } = *self.types().get(info) else { return None };
        let mut dependent = false;
        self.map_type(result, &mut |u, part| {
            dependent |= matches!(*u.types().get(part), Type::Param(b, _) if b == binder);
            None
        });
        dependent.then_some(info)
    }

    /// The info a refinement of `prefix`'s widened type gives term member `name`.
    pub fn term_refinement_named(&mut self, prefix: TypeId, name: &str) -> Option<TypeId> {
        let mut t = self.widen(prefix);
        for _ in 0..MAX_STEPS {
            match self.types().get(t).clone() {
                Type::Refined(_, n, info)
                    if n == name && !matches!(self.types().get(info), Type::Alias(_) | Type::Bounds(..)) =>
                {
                    return Some(info);
                }
                Type::Refined(p, ..) | Type::Annotated(p, _) | Type::Rec(_, p) => t = p,
                _ => match self.dealias(t) {
                    d if d != t => t = d,
                    _ => return None,
                },
            }
        }
        None
    }
}
