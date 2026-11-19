// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Recursive types equal but for the binders of the recursive types they contain.

use super::{Binder, Type, TypeId};
use crate::Universe;

impl Universe {
    /// Determines whether types `a` and `b` are the same but for the binders of the recursive
    /// types in them (dotty's `RecType.iso`), as the two parts of
    /// `({ type T; def make: T }, { type T; def make: T })`. Method types and type lambdas in them
    /// compare by their own binders, so equal ones of two separately typed refinements differ.
    pub fn same_but_rec_binders(&mut self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }
        let (to, from) = (self.rec_binders(a), self.rec_binders(b));
        from.len() == to.len() && self.rec_binders_renamed(b, &from, &to) == a
    }

    /// The binders of the recursive types in `t`, outermost first, in the order of their parts.
    fn rec_binders(&mut self, t: TypeId) -> Vec<Binder> {
        let mut out = Vec::new();
        self.map_type(t, &mut |u, x| {
            if let Type::Rec(b, _) = *u.ty(x) {
                out.push(b);
            }
            None
        });
        out
    }

    /// `t` with each binder of `from` in its recursive types and their `this` replaced by the
    /// binder of `to` at the same index.
    fn rec_binders_renamed(&mut self, t: TypeId, from: &[Binder], to: &[Binder]) -> TypeId {
        let renamed = |b: Binder| from.iter().position(|&f| f == b).map(|i| to[i]);
        self.map_type(t, &mut |u, x| match *u.ty(x) {
            Type::Rec(b, inner) => {
                let b = renamed(b)?;
                let inner = u.rec_binders_renamed(inner, from, to);
                Some(u.intern_type(Type::Rec(b, inner)))
            }
            Type::RecThis(b) => Some(u.intern_type(Type::RecThis(renamed(b)?))),
            _ => None,
        })
    }
}
