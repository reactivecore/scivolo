// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Type parameters in a signature: by name, applied higher-kinded method type parameters (dotty's
//! `resolveAppliedType`), and erasure with the method type parameters in scope bounded.

use super::{Boxing, Sig};
use tasty::{Binder, Erased, Kind, SymbolId, Type, TypeId};

/// What an applied method type parameter stands for in a signature.
enum Applied {
    /// Its bound, a type lambda, applied to the arguments.
    Resolved(TypeId),
    /// The parameter itself, its bound being no type lambda.
    Param(String),
    /// Its erasure, the applied bound referring to the parameter again.
    Erased,
}

impl Sig<'_> {
    /// Appends `t`, `tycon[args]`, if `tycon` is a method type parameter in scope: its bound
    /// applied if that is a reference, else by name or erased; whether it did.
    pub fn applied_param_ref(&mut self, t: TypeId, tycon: TypeId, args: &[TypeId], boxing: Boxing) -> bool {
        match self.applied_param(tycon, args) {
            Some(Applied::Resolved(r)) => {
                let r = self.u.dealias(r);
                match *self.u.ty(r) {
                    Type::Sym(..) | Type::TypeRef(..) | Type::Applied(..) | Type::Param(..) => {
                        self.jsig(r, false, boxing)
                    }
                    _ => self.erased(t, boxing),
                }
            }
            Some(Applied::Param(name)) => self.out.push_str(&format!("T{name};")),
            Some(Applied::Erased) => self.erased(t, boxing),
            None => return false,
        }
        true
    }

    /// `tycon[args]`, if `tycon` is a method type parameter in scope.
    fn applied_param(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<Applied> {
        let (b, i) = self.u.param_ref(tycon)?;
        let (name, bounds) = self.poly_param(b, i)?;
        let hi = self.u.bounds(bounds).map_or(bounds, |(_, hi)| hi);
        let Some(lambda) = self.u.lambda(hi) else { return Some(Applied::Param(name)) };
        let (binder, names, result) = (lambda.binder, lambda.names(), lambda.result);
        let t = self.u.subst(result, binder, &names, args);
        let mut recursive = false;
        self.u.map_type(t, &mut |_, x| {
            recursive |= x == tycon;
            None
        });
        Some(if recursive { Applied::Erased } else { Applied::Resolved(t) })
    }

    /// The name and bounds of method type parameter `i` of `binder`, if in scope.
    pub fn poly_param(&self, binder: Binder, i: u32) -> Option<(String, TypeId)> {
        let (_, params) = self.poly.iter().find(|p| p.0 == binder)?;
        params.get(i as usize).cloned()
    }

    /// Replaces a reference to a parameter of the method by its bounds in `t`, also in the parts
    /// of a union such as `T | Null` and as the upper bound of a wildcard such as `? <: T`.
    pub fn with_param_bounds(&mut self, t: TypeId) -> TypeId {
        if let Some((_, bounds)) = self.u.param_ref(t).and_then(|(b, i)| self.poly_param(b, i)) {
            return bounds;
        }
        match *self.u.ty(t) {
            Type::Or(a, b, soft) => {
                let (a, b) = (self.with_param_bounds(a), self.with_param_bounds(b));
                self.u.intern_type(Type::Or(a, b, soft))
            }
            Type::Bounds(lo, hi) => {
                let hi = self.with_param_bounds(hi);
                let hi = self.u.bounds(hi).map_or(hi, |(_, hi)| hi);
                self.u.intern_type(Type::Bounds(lo, hi))
            }
            _ => t,
        }
    }

    /// The type parameter `t` refers to, if it appears in the signature by name.
    pub fn type_param(&mut self, t: TypeId) -> Option<SymbolId> {
        let s = match *self.u.ty(t) {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.u.resolve_ref(t)?,
            _ => return None,
        };
        let owner = self.u.symbol(s).owner?;
        if self.u.symbol(s).kind != Kind::TypeParam
            || self.u.symbol(owner).kind.is_type() && !self.u.is_class(owner)
        {
            return None;
        }
        let mut o = Some(s);
        while let Some(x) = o {
            if self.tops.contains(&x) || x == self.sym0 || x == self.origin || Some(x) == self.extended {
                return Some(s);
            }
            o = self.u.symbol(x).owner;
        }
        None
    }

    /// Appends the erasure of `t`, the method type parameters in scope replaced by their bounds.
    pub fn erased(&mut self, t: TypeId, boxing: Boxing) {
        let bounds: Vec<(Binder, Vec<TypeId>)> =
            self.poly.iter().map(|(b, ps)| (*b, ps.iter().map(|p| p.1).collect())).collect();
        let t = self.u.upper_bound(t, &bounds);
        let e = self.u.erase(t);
        self.erased_sig(&e, boxing);
    }

    /// Whether a method type parameter of `bounds` erases to a primitive, also through a value
    /// class, and so appears by its erasure.
    pub fn primitive_bound(&mut self, bounds: TypeId) -> bool {
        let hi = self.u.bounds(bounds).map_or(bounds, |(_, hi)| hi);
        match self.u.erase_value(hi) {
            Erased::Value(_, e) => matches!(*e, Erased::Prim(_)),
            e => matches!(e, Erased::Prim(_)),
        }
    }
}
