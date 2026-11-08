//! Whether the match types and compile-time operations of type trees reduce, or stay unreduced
//! as dotty keeps them.

use super::Typer;
use tasty::tags::{FINAL, OBJECT};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Whether compile-time operation `tycon[args]` normalizes or, referring to an abstract type,
    /// a value's singleton type or a wildcard, stays as dotty keeps it.
    pub fn folds(&mut self, tycon: TypeId, args: &[TypeId]) -> bool {
        let op = self.u.intern_type(Type::Applied(tycon, args.to_vec()));
        self.u.try_normalize(op).is_some()
            || self.refers_to_abstract(op, true)
            || args.iter().any(|&a| matches!(self.u.types().get(a), Type::Bounds(..)))
    }

    /// Whether `t` is a match type that refers to no type parameter, parameter or abstract type,
    /// takes no `this` of a class that may be extended, and does not reduce.
    pub fn is_ground_match(&mut self, t: TypeId) -> bool {
        let dealiased = self.u.dealias(t);
        if !matches!(self.u.types().get(dealiased), Type::Match { .. }) || self.u.try_normalize(t).is_some() {
            return false;
        }
        let args = self.u.applied(t).map(|(_, args)| args.to_vec()).unwrap_or_default();
        let open_this = args.into_iter().any(|a| match *self.u.types().get(a) {
            Type::This(c) => self.u.class_symbol(c).is_some_and(|c| {
                let flags = self.u.symbol(c).flags;
                !flags.has(OBJECT) && !flags.has(FINAL)
            }),
            _ => false,
        });
        !open_this && !self.refers_to_abstract(t, false)
    }

    /// Whether `t` refers to a type parameter, a parameter or an abstract type, or a value if
    /// `values`.
    fn refers_to_abstract(&mut self, t: TypeId, values: bool) -> bool {
        let mut abstract_part = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = *u.types().get(part) {
                abstract_part |= match u.symbol(s).kind {
                    Kind::TypeParam | Kind::Param => true,
                    Kind::Val => values,
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
