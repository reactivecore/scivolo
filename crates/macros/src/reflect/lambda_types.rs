// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! `MethodType.apply`, `PolyType.apply` and `TypeLambda.apply`, whose parameter and result types
//! may refer to the parameters, and `param` of a lambda type.

use super::Fallible;
use crate::host::Host;
use tasty::{Type, TypeId};

/// Lambda kinds: method types plain, implicit and contextual, polymorphic types, type lambdas.
const POLY: i32 = 3;
const LAMBDA: i32 = 4;

impl Host {
    /// A lambda type of kind `kind` without parameters, whose binder `lambda_type` gives the type
    /// it makes.
    pub fn lambda_binder(&mut self, kind: i32) -> TypeId {
        let (binder, params, result) =
            (self.u.fresh_binder(), Vec::new(), self.u.intern_type(Type::NoPrefix));
        self.u.intern_type(match kind {
            POLY => Type::Poly { binder, params, result },
            LAMBDA => Type::Lambda { binder, params, result },
            _ => Type::Method { binder, params, result, implicit: kind != 0, contextual: kind == 2 },
        })
    }

    /// The lambda type of `binder`'s kind and binder with parameters of `names` and types
    /// `types` but the last, and the last as its result type.
    pub fn lambda_type(&mut self, binder: TypeId, types: Vec<TypeId>, names: String) -> Fallible<TypeId> {
        let (&result, params) = types.split_last().ok_or("a lambda type without its result type")?;
        let names: Vec<String> = names.split('\0').filter(|n| !n.is_empty()).map(str::to_owned).collect();
        if names.len() != params.len() {
            return Err("a lambda type with another number of names than types");
        }
        let params: Vec<(String, TypeId)> = names.into_iter().zip(params.iter().copied()).collect();
        let t = match self.u.types().get(binder).clone() {
            Type::Method { binder, implicit, contextual, .. } => {
                Type::Method { binder, params, result, implicit, contextual }
            }
            Type::Poly { binder, .. } => Type::Poly { binder, params, result },
            Type::Lambda { binder, .. } => Type::Lambda { binder, params, result },
            _ => return Err("a lambda type of another binder"),
        };
        Ok(self.u.intern_type(t))
    }

    /// The reference to parameter `i` of lambda type `t` (dotty's `newParamRef`).
    pub fn param_ref(&mut self, t: TypeId, i: i32) -> Fallible<TypeId> {
        let binder = match *self.u.types().get(t) {
            Type::Method { binder, .. } | Type::Poly { binder, .. } | Type::Lambda { binder, .. } => binder,
            _ => return Err("param of a type other than a lambda type"),
        };
        Ok(self.u.intern_type(Type::Param(binder, i as u32)))
    }
}
