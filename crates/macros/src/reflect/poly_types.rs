// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! `PolyType.apply`, `PolyType.param` and `TypeBounds.apply`: a polymorphic type is made from a
//! placeholder, a polymorphic type without parameters whose binder the parameter references name.

use super::Fallible;
use crate::host::Host;
use tasty::{ANY, NOTHING, Type, TypeId};

impl Host {
    /// A placeholder of a new polymorphic type.
    pub fn poly_binder(&mut self) -> TypeId {
        let binder = self.u.fresh_binder();
        self.u.intern_type(Type::Poly { binder, params: Vec::new(), result: NOTHING })
    }

    /// The reference to parameter `idx` of polymorphic type or placeholder `t`.
    pub fn poly_param(&mut self, t: TypeId, idx: i32) -> Fallible<TypeId> {
        let Type::Poly { binder, params, .. } = self.u.types().get(t) else {
            return Err("PolyType.param of another type");
        };
        let (binder, n) = (*binder, params.len());
        let idx = u32::try_from(idx).ok().filter(|&i| n == 0 || (i as usize) < n);
        let idx = idx.ok_or("PolyType.param of a parameter it does not have")?;
        Ok(self.u.intern_type(Type::Param(binder, idx)))
    }

    /// The polymorphic type of placeholder `placeholder` with the parameter bounds and the result
    /// type `types`, the parameters named by `names` separated by `\0`.
    pub fn poly_type(&mut self, placeholder: TypeId, types: Vec<TypeId>, names: String) -> Fallible<TypeId> {
        let Type::Poly { binder, .. } = *self.u.types().get(placeholder) else {
            return Err("PolyType.apply without its placeholder");
        };
        let (&result, bounds) = types.split_last().ok_or("PolyType.apply without its result type")?;
        let names: Vec<String> = names.split('\0').filter(|n| !n.is_empty()).map(str::to_owned).collect();
        if names.len() != bounds.len() || names.is_empty() {
            return Err("PolyType.apply with no or another number of names than bounds");
        }
        if bounds.iter().any(|&b| !matches!(self.u.types().get(b), Type::Bounds(..))) {
            return Err("PolyType.apply with parameter bounds other than TypeBounds");
        }
        let params = names.into_iter().zip(bounds.iter().copied()).collect();
        Ok(self.u.intern_type(Type::Poly { binder, params, result }))
    }

    /// `TypeBounds(low, hi)` of the types numbered `low` and `hi`, `Nothing` and `Any` for `-1`.
    pub fn type_bounds(&mut self, low: i32, hi: i32) -> Fallible<TypeId> {
        let low = if low < 0 { NOTHING } else { self.type_of(low as u32)? };
        let hi = if hi < 0 { ANY } else { self.type_of(hi as u32)? };
        Ok(self.u.intern_type(Type::Bounds(low, hi)))
    }
}
