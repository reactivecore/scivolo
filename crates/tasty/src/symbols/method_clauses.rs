//! The parameter clauses and result of method types.

use super::Universe;
use crate::types::{Binder, NameId, Type, TypeId};

/// The binder and parameters of each value parameter clause of a method type.
pub type Clauses = Vec<(Binder, Vec<(NameId, TypeId)>)>;

impl Universe {
    /// The binder and parameters of each value parameter clause of method type `t`.
    pub fn method_clauses(&self, mut t: TypeId) -> Clauses {
        let mut out = Vec::new();
        while let Type::Poly { result, .. } | Type::Method { result, .. } = *self.ty(t) {
            if let Type::Method { binder, params, .. } = *self.ty(t) {
                out.push((binder, self.params_of(params).to_vec()));
            }
            t = result;
        }
        out
    }

    /// The result type of method type `t` after all its clauses.
    pub fn result_type(&self, mut t: TypeId) -> TypeId {
        while let Type::Poly { result, .. } | Type::Method { result, .. } = *self.ty(t) {
            t = result;
        }
        t
    }

    /// The binder of each type-parameter clause of method type `t` and its number of parameters.
    pub fn poly_binders(&self, mut t: TypeId) -> Vec<(Binder, usize)> {
        let mut out = Vec::new();
        while let Type::Poly { result, .. } | Type::Method { result, .. } = *self.ty(t) {
            if let Type::Poly { binder, params, .. } = *self.ty(t) {
                out.push((binder, self.params_of(params).len()));
            }
            t = result;
        }
        out
    }

    /// The value parameter clauses of method type `t`: whether each is implicit, and its
    /// parameters' types.
    pub fn param_clauses(&self, mut t: TypeId) -> Vec<(bool, Vec<TypeId>)> {
        let mut out = Vec::new();
        loop {
            match *self.ty(t) {
                Type::Poly { result, .. } => t = result,
                Type::Method { params, result, implicit, .. } => {
                    out.push((implicit, self.params_of(params).iter().map(|(_, p)| *p).collect()));
                    t = result;
                }
                _ => return out,
            }
        }
    }
}
