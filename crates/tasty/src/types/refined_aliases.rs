// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Type members replaced by the aliases refinements of their prefixes give them.

use super::{Type, TypeId};
use crate::{Kind, Universe};

/// How many steps dealiasing through refinements takes at most.
const MAX_STEPS: u32 = 64;

impl Universe {
    /// The alias the outermost alias refinement of `name` in `t` gives it, through skolems,
    /// annotations and type aliases (dotty's `lookupRefined`).
    pub fn lookup_refined(&mut self, t: TypeId, name: &str) -> Option<TypeId> {
        match self.ty(t).clone() {
            Type::Refined(p, n, info) => match *self.ty(info) {
                Type::Alias(alias) if self.name(n) == name => Some(alias),
                _ => self.lookup_refined(p, name),
            },
            Type::Annotated(p, _) | Type::Skolem(_, p) => self.lookup_refined(p, name),
            Type::Rec(binder, p) => {
                let candidate = self.lookup_refined(p, name)?;
                let mut refers = false;
                self.map_type(candidate, &mut |u, x| {
                    refers |= *u.ty(x) == Type::RecThis(binder);
                    None
                });
                (!refers).then_some(candidate)
            }
            Type::TypeRef(..) | Type::Sym(..) if self.dealias(t) != t => {
                let d = self.dealias(t);
                self.lookup_refined(d, name)
            }
            _ => None,
        }
    }

    /// `t` with its type members selected on `prefix`, a skolem or a refined type, replaced by
    /// the alias a refinement of `prefix` gives them (dotty's `derivedSelect`).
    pub fn refined_selects(&mut self, t: TypeId, prefix: TypeId) -> TypeId {
        if !matches!(self.ty(prefix), Type::Skolem(..) | Type::Refined(..) | Type::Rec(..)) {
            return t;
        }
        self.map_type(t, &mut |u, x| {
            let Type::Sym(Some(p), s) = *u.ty(x) else { return None };
            if p != prefix || u.symbol(s).kind != Kind::Type {
                return None;
            }
            let name = u.symbol(s).name.clone();
            u.lookup_refined(prefix, &name)
        })
    }

    /// `t`, a type member a refinement of its prefix aliases replaced by the alias, repeatedly.
    pub fn refinement_dealiased(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..MAX_STEPS {
            let Type::Sym(Some(pre), s) = *self.ty(t) else { break };
            let Some(info) = self.refinement_info(pre, s) else { break };
            match *self.ty(info) {
                Type::Alias(alias) if alias != t => t = alias,
                _ => break,
            }
        }
        t
    }

    /// `t` with its references to type members that refinements of `refined` alias replaced by
    /// their aliases.
    pub fn with_refined_aliases(&mut self, t: TypeId, refined: TypeId) -> TypeId {
        if !matches!(self.ty(refined), Type::Refined(..)) {
            return t;
        }
        self.map_type(t, &mut |u, part| {
            let Type::Sym(Some(_), s) = *u.ty(part) else { return None };
            let info = u.refinement_info(refined, s)?;
            u.type_alias(info)
        })
    }
}
