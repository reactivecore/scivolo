//! The type parameters of classes and methods, and the type of a class's `this`.

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// The type parameters of class or method `c`, in order.
    pub fn type_params(&self, c: SymbolId) -> Vec<SymbolId> {
        self.declared_type_params.get(&c).cloned().unwrap_or_default()
    }

    /// The type of `this` in class `c`: `c` applied to its own type parameters, seen from `this`
    /// (dotty's `appliedRef`).
    pub fn this_type(&mut self, c: SymbolId) -> TypeId {
        if let Some(&t) = self.this_types.get(&c) {
            return t;
        }
        let class = self.intern_type(Type::Sym(None, c));
        let this = self.intern_type(Type::This(class));
        let params: Vec<TypeId> =
            self.type_params(c).into_iter().map(|p| self.intern_type(Type::Sym(Some(this), p))).collect();
        let t = match params.is_empty() {
            true => class,
            false => self.intern_applied(class, &params),
        };
        self.this_types.insert(c, t);
        t
    }

    /// The class type parameter reference `t` names: its class and index.
    pub fn class_param(&mut self, t: TypeId) -> Option<(SymbolId, usize)> {
        let s = match *self.ty(t) {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.resolve_ref(t)?,
            _ => return None,
        };
        let owner = self.symbol(s).owner?;
        if self.symbol(s).kind != Kind::TypeParam || self.symbol(owner).kind != Kind::Class {
            return None;
        }
        Some((owner, self.declared_type_params.get(&owner)?.iter().position(|&p| p == s)?))
    }
}
