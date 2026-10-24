//! Whether the match types and compile-time operations of type trees reduce, or stay unreduced
//! as dotty keeps them.

use super::Typer;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Whether compile-time operation `tycon[args]` normalizes or, referring to an abstract type
    /// or a wildcard, stays as dotty keeps it.
    pub fn folds(&mut self, tycon: TypeId, args: &[TypeId]) -> bool {
        let op = self.u.intern_type(Type::Applied(tycon, args.to_vec()));
        self.u.try_normalize(op).is_some()
            || self.refers_to_abstract(op)
            || args.iter().any(|&a| matches!(self.u.types().get(a), Type::Bounds(..)))
    }

    /// Whether `t` is a match type that refers to no type parameter, parameter or abstract type
    /// and does not reduce.
    pub fn is_ground_match(&mut self, t: TypeId) -> bool {
        let dealiased = self.u.dealias(t);
        if !matches!(self.u.types().get(dealiased), Type::Match { .. }) || self.u.try_normalize(t).is_some() {
            return false;
        }
        !self.refers_to_abstract(t)
    }

    /// Whether `t` refers to a type parameter, a parameter or an abstract type.
    fn refers_to_abstract(&mut self, t: TypeId) -> bool {
        let mut abstract_part = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = *u.types().get(part) {
                abstract_part |= match u.symbol(s).kind {
                    Kind::TypeParam | Kind::Param => true,
                    Kind::Type => {
                        let info = u.info(s);
                        matches!(u.types().get(info), Type::Bounds(..))
                    }
                    _ => false,
                };
            }
            None
        });
        abstract_part
    }
}
