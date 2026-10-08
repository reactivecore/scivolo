//! Singleton types, their normal forms as atoms, and the atoms of singletons and named types.

use super::atoms::Atoms;
use super::refs::Meaning;
use super::{Cmp, MAX_DEPTH};
use crate::tags::OBJECT;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId};

impl Cmp<'_> {
    /// The atoms of singleton `t`: those of its underlying type if it has just one, else `t`
    /// normalized if it is stable.
    pub fn singleton_atoms(&mut self, t: TypeId, depth: u32) -> Atoms {
        let underlying = match self.u.widen_once(t) {
            Some(w) => self.atoms_at(w, depth),
            None => Atoms::Unmodelled,
        };
        match underlying {
            Atoms::Known(r) if self.u.unions().one(&r.hi).is_some() => Atoms::Known(r),
            Atoms::Unknown if self.is_stable(t) && !self.is_unstable_ref(t) => {
                Atoms::single(self.normalized(t, depth))
            }
            other => other.upper(),
        }
    }

    /// Whether `t` refers to a variable, a method that is not stable or a by-name parameter.
    fn is_unstable_ref(&mut self, t: TypeId) -> bool {
        let sym = self.named(t).and_then(|n| n.sym);
        sym.is_some_and(|s| self.u.symbol(s).kind.is_term() && self.is_unstable(s))
    }

    /// The atoms of named type `t`: those below its upper bound, its object for the class of an
    /// object.
    pub fn named_atoms(&mut self, t: TypeId, depth: u32) -> Atoms {
        let Some(n) = self.named(t) else { return Atoms::Unmodelled };
        match n.meaning {
            Meaning::Abstract(_, hi) => self.atoms_at(hi, depth).upper(),
            Meaning::Class(c) if self.u.symbol(c).flags.has(OBJECT) => match self.module_of(c) {
                Some(m) => {
                    let module = self.u.intern_type(Type::Sym(n.prefix, m));
                    Atoms::single(self.normalized(module, depth))
                }
                None => Atoms::Unmodelled,
            },
            Meaning::Class(_) => Atoms::Unknown,
            _ => Atoms::Unmodelled,
        }
    }

    /// The atoms of `tycon[args]`: those below the upper bound of an abstract `tycon`.
    pub fn applied_atoms(&mut self, tycon: TypeId, args: &[TypeId], depth: u32) -> Atoms {
        match self.named(tycon).map(|n| n.meaning) {
            Some(Meaning::Abstract(_, hi)) => {
                let upper = self.apply(hi, args);
                self.atoms_at(upper, depth).upper()
            }
            Some(Meaning::Class(_)) => Atoms::Unknown,
            _ => Atoms::Unmodelled,
        }
    }

    /// Whether `t` is a path, a `this` or a constant.
    pub fn is_singleton(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::TermRef(..) | Type::This(_) | Type::RecThis(_) | Type::Constant(..) | Type::Str(_) => true,
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            _ => false,
        }
    }

    /// Singleton `t` as the singleton its underlying type is, if it is one, else in one form per
    /// value: a static term or object without prefix, another term with its prefix normalized or
    /// `this` of its owner, `this` of a class by its symbol.
    pub fn normalized(&mut self, t: TypeId, depth: u32) -> TypeId {
        if depth >= MAX_DEPTH || !self.is_singleton(t) {
            return t;
        }
        if let Some(w) = self.u.widen_once(t) {
            let w = self.u.dealias(w);
            if self.is_singleton(w) {
                return self.normalized(w, depth + 1);
            }
        }
        let (prefix, s) = match self.u.types().get(t).clone() {
            Type::TermRef(p, name) => match self.u.resolve(p, &name, false) {
                Some(s) => (Some(p), s),
                None => return t,
            },
            Type::Sym(p, s) => (p, s),
            Type::This(c) => {
                let Some(class) = self.u.class_symbol(c) else { return t };
                if let Some(module) = self.static_module(class) {
                    return self.u.intern_type(Type::Sym(None, module));
                }
                let class = self.u.intern_type(Type::Sym(None, class));
                return self.u.intern_type(Type::This(class));
            }
            _ => return t,
        };
        let owner = self.u.symbol(s).owner;
        let prefix = match (prefix, owner) {
            (_, Some(o)) if self.is_static_owner(o) => None,
            (Some(p), _) if !matches!(self.u.types().get(p), Type::NoPrefix) => {
                Some(self.normalized(p, depth + 1))
            }
            (_, Some(o)) if self.u.symbol(o).kind == Kind::Class => {
                let class = self.u.intern_type(Type::Sym(None, o));
                Some(self.u.intern_type(Type::This(class)))
            }
            _ => None,
        };
        self.u.intern_type(Type::Sym(prefix, s))
    }

    /// Whether `o` is a package or the class of an object whose owner is one of these.
    fn is_static_owner(&mut self, o: SymbolId) -> bool {
        match self.u.symbol(o).kind {
            Kind::Package => true,
            Kind::Class if self.u.symbol(o).flags.has(OBJECT) => {
                self.u.symbol(o).owner.is_some_and(|owner| self.is_static_owner(owner))
            }
            _ => false,
        }
    }

    /// The object whose class is `class`, if its owner is static.
    fn static_module(&mut self, class: SymbolId) -> Option<SymbolId> {
        let owner = self.u.symbol(class).owner?;
        if !self.is_static_owner(owner) {
            return None;
        }
        self.module_of(class)
    }

    /// The object whose class is `class`.
    fn module_of(&mut self, class: SymbolId) -> Option<SymbolId> {
        let owner = self.u.symbol(class).owner?;
        if !self.u.symbol(class).flags.has(OBJECT) {
            return None;
        }
        let name = self.u.symbol(class).name.strip_suffix('$')?.to_owned();
        let found = self.u.decl(owner, &name).ok()?;
        found.into_iter().find(|&s| self.u.symbol(s).kind == Kind::Val && self.u.symbol(s).flags.has(OBJECT))
    }
}
