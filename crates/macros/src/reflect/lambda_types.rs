// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! `MethodType.apply`, `PolyType.apply` and `TypeLambda.apply`, whose parameter and result types
//! may refer to the parameters, `param` of a lambda type and `erasedParams` of a method type.

use super::Fallible;
use crate::host::Host;
use tasty::{NameId, Type, TypeId};

/// Lambda kinds: method types plain, implicit and contextual, polymorphic types, type lambdas.
const POLY: i32 = 3;
const LAMBDA: i32 = 4;

impl Host {
    /// A lambda type of kind `kind` without parameters, whose binder `lambda_type` gives the type
    /// it makes.
    pub fn lambda_binder(&mut self, kind: i32) -> TypeId {
        let (binder, result) = (self.u.fresh_binder(), self.u.intern_type(Type::NoPrefix));
        match kind {
            POLY => self.u.intern_poly(binder, &[], result),
            LAMBDA => self.u.intern_lambda(binder, &[], result),
            _ => self.u.intern_method(binder, &[], result, kind != 0, kind == 2),
        }
    }

    /// The lambda type of `binder`'s kind and binder with parameters of `names` and types
    /// `types` but the last, and the last as its result type.
    pub fn lambda_type(&mut self, binder: TypeId, types: Vec<TypeId>, names: String) -> Fallible<TypeId> {
        let (&result, params) = types.split_last().ok_or("a lambda type without its result type")?;
        let names: Vec<NameId> =
            names.split('\0').filter(|n| !n.is_empty()).map(|n| self.u.intern_name(n)).collect();
        if names.len() != params.len() {
            return Err("a lambda type with another number of names than types");
        }
        let params: Vec<(NameId, TypeId)> = names.into_iter().zip(params.iter().copied()).collect();
        Ok(match *self.u.ty(binder) {
            Type::Method { binder, implicit, contextual, .. } => {
                self.u.intern_method(binder, &params, result, implicit, contextual)
            }
            Type::Poly { binder, .. } => self.u.intern_poly(binder, &params, result),
            Type::Lambda { binder, .. } => self.u.intern_lambda(binder, &params, result),
            _ => return Err("a lambda type of another binder"),
        })
    }

    /// The reference to parameter `i` of lambda type `t` (dotty's `newParamRef`).
    pub fn param_ref(&mut self, t: TypeId, i: i32) -> Fallible<TypeId> {
        let binder = match *self.u.ty(t) {
            Type::Method { binder, .. } | Type::Poly { binder, .. } | Type::Lambda { binder, .. } => binder,
            _ => return Err("param of a type other than a lambda type"),
        };
        Ok(self.u.intern_type(Type::Param(binder, i as u32)))
    }

    /// Whether each parameter of method type `t` is erased (dotty's `paramErasureStatuses`).
    pub fn erased_params(&mut self, t: TypeId) -> Vec<i32> {
        let params = self.u.method(t).map(|m| m.param_types()).unwrap_or_default();
        params.into_iter().map(|p| self.u.is_erased_param(p) as i32).collect()
    }
}
