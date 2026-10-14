//! The types of implicit candidates as their ranking compares them: givens with classes of their
//! own and given objects as their parents (dotty's `widenGiven` and `prepare` in `compare`).

use super::super::Typer;
use super::candidates::Candidate;
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type of candidate `c` as seen from its prefix, a by-name result widened, the result of
    /// a given with a class of its own widened to the class's parents.
    pub fn alt_type(&mut self, c: Candidate) -> TypeId {
        self.complete(c.sym);
        let tpe = self.ref_type(c.prefix, c.sym);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let info = match *self.u.types().get(info) {
            Type::ByName(r) => r,
            _ => info,
        };
        self.widen_given(info, c.sym)
    }

    fn widen_given(&mut self, t: TypeId, alt: SymbolId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::Method { binder, params, result, implicit: true, contextual } => {
                let result = self.widen_given(result, alt);
                self.u.intern_type(Type::Method { binder, params, result, implicit: true, contextual })
            }
            Type::Poly { binder, params, result } => {
                let result = self.widen_given(result, alt);
                self.u.intern_type(Type::Poly { binder, params, result })
            }
            _ => match self.u.class_symbol(t) {
                Some(c) if self.is_co_defined_given(alt, c) => self.parents_of(t, c),
                _ => t,
            },
        }
    }

    /// Whether `alt` is the method of a given or implicit class `cls` (dotty's
    /// `isCoDefinedGiven`).
    fn is_co_defined_given(&self, alt: SymbolId, cls: SymbolId) -> bool {
        let (a, k) = (self.u.symbol(alt), self.u.symbol(cls));
        let given = |f: &tasty::Flags| f.has(GIVEN) || f.has(IMPLICIT);
        a.kind == Kind::Def && given(&a.flags) && given(&k.flags) && a.name == k.name && a.owner == k.owner
    }

    /// `t`, or if it is the class of a given object, that class's parents.
    pub fn prepared(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        let Type::Sym(_, c) = *self.u.types().get(t) else { return t };
        let given_object = self.u.is_module_class(c)
            && self.u.source_module(c).is_some_and(|m| self.u.symbol(m).flags.has(GIVEN));
        if given_object { self.parents_of(t, c) } else { t }
    }

    /// The intersection of the parents of class `c` as base types of `t` (dotty's
    /// `widenToParents`).
    pub fn parents_of(&mut self, t: TypeId, c: SymbolId) -> TypeId {
        let mut out: Option<TypeId> = None;
        for p in self.u.parents(c) {
            let base = self.u.class_symbol(p).and_then(|pc| self.u.base_type(t, pc)).unwrap_or(p);
            out = Some(out.map_or(base, |o| self.u.intern_type(Type::And(o, base))));
        }
        out.unwrap_or(t)
    }
}
