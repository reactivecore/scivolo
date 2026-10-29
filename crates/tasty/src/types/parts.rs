// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Borrowed views of the parts of binding and applied types.

use super::{Binder, Type, TypeId};
use crate::Universe;

/// The parts a `Method`, `Poly` or `Lambda` type binds; `implicit` and `contextual` are false
/// for the latter two.
#[derive(Clone, Copy)]
pub struct Binding<'a> {
    pub binder: Binder,
    pub params: &'a [(String, TypeId)],
    pub result: TypeId,
    pub implicit: bool,
    pub contextual: bool,
}

impl Binding<'_> {
    pub fn param_types(&self) -> Vec<TypeId> {
        self.params.iter().map(|(_, t)| *t).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.params.iter().map(|(n, _)| n.clone()).collect()
    }
}

impl Universe {
    pub fn method(&self, t: TypeId) -> Option<Binding<'_>> {
        match self.types().get(t) {
            Type::Method { binder, params, result, implicit, contextual } => Some(Binding {
                binder: *binder,
                params,
                result: *result,
                implicit: *implicit,
                contextual: *contextual,
            }),
            _ => None,
        }
    }

    pub fn poly(&self, t: TypeId) -> Option<Binding<'_>> {
        match self.types().get(t) {
            Type::Poly { binder, params, result } => Some(plain(*binder, params, *result)),
            _ => None,
        }
    }

    pub fn lambda(&self, t: TypeId) -> Option<Binding<'_>> {
        match self.types().get(t) {
            Type::Lambda { binder, params, result } => Some(plain(*binder, params, *result)),
            _ => None,
        }
    }

    pub fn applied(&self, t: TypeId) -> Option<(TypeId, &[TypeId])> {
        match self.types().get(t) {
            Type::Applied(tycon, args) => Some((*tycon, args)),
            _ => None,
        }
    }

    /// The result of a `Poly` type, else `t` (one layer of dotty's `stripPoly`).
    pub fn strip_poly(&self, t: TypeId) -> TypeId {
        match *self.types().get(t) {
            Type::Poly { result, .. } => result,
            _ => t,
        }
    }
}

fn plain(binder: Binder, params: &[(String, TypeId)], result: TypeId) -> Binding<'_> {
    Binding { binder, params, result, implicit: false, contextual: false }
}
