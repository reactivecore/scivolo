//! Erasure of intersections in the signatures of Scala 2 symbols (dotty's `Scala2Erasure`).

use super::Erased;
use crate::tags::TRAIT;
use crate::types::{Type, TypeId};
use crate::{SymbolId, Universe};

/// What stands for a type's symbol in Scala 2: a symbol, or an intersection, refinement or
/// structural member, which Scala 2 gives a symbol of its own.
#[derive(Clone, Copy, PartialEq)]
enum Pseudo {
    Sym(SymbolId),
    Type(TypeId),
}

impl Universe {
    /// The erasure of intersection `t` in Scala 2: of its dominator.
    pub fn erase_scala2_and(&mut self, t: TypeId) -> Erased {
        let mut parents = Vec::new();
        self.flatten(t, &mut parents);
        let dominator = self.dominator(&parents);
        self.erase(dominator)
    }

    /// The parents of intersection `t`, nested ones flattened, in order.
    fn flatten(&mut self, t: TypeId, out: &mut Vec<TypeId>) {
        let dealiased = self.dealias(t);
        match *self.types().get(dealiased) {
            Type::And(a, b) => {
                self.flatten(a, out);
                self.flatten(b, out);
            }
            _ => out.push(t),
        }
    }

    /// Scala 2's `intersectionDominator`: an array of the elements' dominator if one parent is an
    /// array, else the first class or refinement no other parent derives from, else the first such
    /// trait.
    fn dominator(&mut self, parents: &[TypeId]) -> TypeId {
        let psyms: Vec<Pseudo> = parents.iter().map(|&p| self.pseudo(p)).collect();
        let dealiased: Vec<TypeId> = parents.iter().map(|&p| self.dealias(p)).collect();
        let arrays: Vec<TypeId> = dealiased.into_iter().filter(|&p| self.is_array(p)).collect();
        if let Some(&first) = arrays.first() {
            let elements: Vec<TypeId> = arrays
                .iter()
                .filter_map(|&p| match self.types().get(p) {
                    Type::Applied(_, args) => args.first().copied(),
                    _ => None,
                })
                .collect();
            let Type::Applied(tycon, _) = *self.types().get(first) else { unreachable!("an array type") };
            let element = self.dominator(&elements);
            return self.intern_type(Type::Applied(tycon, vec![element]));
        }
        let unshadowed: Vec<bool> = psyms
            .iter()
            .map(|&p| !psyms.iter().any(|&q| q != p && self.is_non_bottom_subclass(q, p)))
            .collect();
        let class = (0..parents.len()).find(|&i| {
            unshadowed[i]
                && match psyms[i] {
                    Pseudo::Sym(s) => self.is_class(s) && !self.symbol(s).flags.has(TRAIT),
                    Pseudo::Type(t) => matches!(self.types().get(t), Type::And(..) | Type::Refined(..)),
                }
        });
        let first = class.or_else(|| unshadowed.iter().position(|&u| u)).unwrap_or(0);
        parents[first]
    }

    /// The pseudo symbol of `t`.
    fn pseudo(&mut self, t: TypeId) -> Pseudo {
        let widened = self.widen(t);
        let t = self.dealias(widened);
        match self.types().get(t).clone() {
            Type::And(..) | Type::Refined(..) => Pseudo::Type(t),
            Type::Sym(..) | Type::TypeRef(..) => self.referenced(t).map_or(Pseudo::Type(t), Pseudo::Sym),
            Type::Applied(u, _)
            | Type::Annotated(u, _)
            | Type::Rec(_, u)
            | Type::Bounds(_, u)
            | Type::Lambda { result: u, .. } => self.pseudo(u),
            Type::Or(..) => match self.erase(t) {
                Erased::Class(name) => {
                    let r = self.class_ref(&name);
                    self.pseudo(r)
                }
                _ => Pseudo::Type(t),
            },
            _ => Pseudo::Type(t),
        }
    }

    /// Scala 2's `isNonBottomSubClass` on pseudo symbols, as dotty emulates it.
    fn is_non_bottom_subclass(&mut self, p: Pseudo, that: Pseudo) -> bool {
        if p == that {
            return true;
        }
        if let Pseudo::Type(t) = that
            && matches!(self.types().get(t), Type::And(..) | Type::Refined(..))
        {
            return false;
        }
        match p {
            Pseudo::Sym(a) => match that {
                Pseudo::Sym(b) if self.is_class(a) && self.is_class(b) => self.derives_from(a, b),
                Pseudo::Sym(_) if self.is_class(a) => false,
                _ => self.upper_bound_is_subclass(a, that),
            },
            Pseudo::Type(t) => match self.types().get(t).clone() {
                Type::Refined(parent, _, _) => {
                    let q = self.pseudo(parent);
                    self.is_non_bottom_subclass(q, that)
                }
                Type::And(a, b) => {
                    let (qa, qb) = (self.pseudo(a), self.pseudo(b));
                    self.is_non_bottom_subclass(qa, that) || self.is_non_bottom_subclass(qb, that)
                }
                _ => false,
            },
        }
    }

    /// Whether abstract type `s` has an upper bound whose pseudo symbol is a subclass of `that`.
    fn upper_bound_is_subclass(&mut self, s: SymbolId, that: Pseudo) -> bool {
        let mut info = self.info(s);
        while let Type::Lambda { result, .. } = *self.types().get(info) {
            info = result;
        }
        let Type::Bounds(_, hi) = *self.types().get(info) else { return false };
        let q = self.pseudo(hi);
        self.is_non_bottom_subclass(q, that)
    }
}
