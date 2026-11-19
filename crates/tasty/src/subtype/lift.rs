// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Comparisons retried with references to an enclosing object with opaque types made its
//! `this` (dotty's `liftToThis`, `tryLiftedToThis1`, `tryLiftedToThis2`).

use super::Cmp;
use crate::Universe;
use crate::types::{Type, TypeId};

impl Cmp<'_> {
    /// `lift(a) <: b` if lifting changes `a`.
    pub fn lifted_left(&mut self, a: TypeId, b: TypeId) -> bool {
        let lifted = self.u.lift_to_this(a);
        lifted != a && self.recur(lifted, b) && self.used()
    }

    /// `a <: lift(b)` if lifting changes `b`.
    pub fn lifted_right(&mut self, a: TypeId, b: TypeId) -> bool {
        let lifted = self.u.lift_to_this(b);
        lifted != b && self.recur(a, lifted) && self.used()
    }

    /// Counts a success through a lifted reference; true.
    fn used(&mut self) -> bool {
        self.u.count_opaque_lift();
        true
    }
}

impl Universe {
    /// Makes a reference to an object with opaque types that encloses the comparison owner, as
    /// a path or a prefix, the `this` of its class: `o.T` inside `object o` becomes `o.this.T`
    /// (dotty's `TypeComparer.liftToThis`).
    pub fn lift_to_this(&mut self, t: TypeId) -> TypeId {
        let module = match *self.ty(t) {
            Type::Sym(_, s) if self.symbol(s).kind.is_term() => Some(s),
            Type::TermRef(..) => self.resolve_ref(t),
            Type::Sym(Some(p), s) => {
                let q = self.lift_to_this(p);
                return if q == p { t } else { self.intern_type(Type::Sym(Some(q), s)) };
            }
            Type::TypeRef(p, n) => {
                let q = self.lift_to_this(p);
                return if q == p { t } else { self.intern_type(Type::TypeRef(q, n)) };
            }
            Type::Applied(tycon, args) => {
                let lifted = self.lift_to_this(tycon);
                return if lifted == tycon { t } else { self.intern_type(Type::Applied(lifted, args)) };
            }
            Type::Annotated(p, a) => {
                let q = self.lift_to_this(p);
                if q == p {
                    return t;
                }
                let lifted = self.intern_type(Type::Annotated(q, a));
                return self.carry_annotation_tree(t, lifted);
            }
            _ => None,
        };
        match module.and_then(|m| self.enclosing_opaque_module(m)) {
            Some(class) => {
                let class = self.intern_type(Type::Sym(None, class));
                self.intern_type(Type::This(class))
            }
            None => t,
        }
    }
}
