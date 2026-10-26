//! Type lambdas and polymorphic method types: parameters renamed to a common binder, bounds
//! compared contravariantly, then the bodies.

use super::Cmp;
use crate::types::{Binder, Type, TypeId};
use crate::{Kind, Universe};

/// A binding type: its binder, its parameters' names and bounds, its body.
pub type Binding<'a> = (Binder, &'a [(String, TypeId)], TypeId);

impl Cmp<'_> {
    /// Whether binding type `a` is a subtype of `b`: as many parameters, each bound of `b`'s
    /// within `a`'s, `a`'s body a subtype of `b`'s.
    pub fn compare_binders(&mut self, a: Binding, b: Binding) -> bool {
        if a.1.len() != b.1.len() {
            return false;
        }
        let common = Binder(Universe::COMPARED_BINDERS, self.depth);
        let refs: Vec<TypeId> =
            (0..a.1.len() as u32).map(|i| self.u.intern_type(Type::Param(common, i))).collect();
        let bounds_ok = a.1.iter().zip(b.1).all(|(&(_, bound_a), &(_, bound_b))| {
            let (bound_a, bound_b) = (self.renamed(a, bound_a, &refs), self.renamed(b, bound_b, &refs));
            let (bound_a, bound_b) = (self.lambda_bounds(bound_a), self.lambda_bounds(bound_b));
            self.recur(bound_b, bound_a)
        });
        let (body_a, body_b) = (self.renamed(a, a.2, &refs), self.renamed(b, b.2, &refs));
        bounds_ok && self.recur(body_a, body_b)
    }

    /// `t` with its references to the symbols of term parameters named as `params`, the clause of
    /// binder `binder`, replaced by parameter references.
    pub fn param_syms_as_refs(&mut self, t: TypeId, params: &[(String, TypeId)], binder: Binder) -> TypeId {
        self.u.map_type(t, &mut |u, x| {
            let Type::Sym(None, s) = *u.types().get(x) else { return None };
            if u.symbol(s).kind != Kind::Param {
                return None;
            }
            let i = params.iter().position(|(n, _)| *n == u.symbol(s).name)?;
            Some(u.intern_type(Type::Param(binder, i as u32)))
        })
    }

    /// Higher-kinded bounds `[X] =>> >: L <: H` as `>: [X] =>> L <: [X] =>> H`, `Nothing` as
    /// the lower bound for a lower bound `Nothing`.
    fn lambda_bounds(&mut self, t: TypeId) -> TypeId {
        let Type::Lambda { binder, ref params, result } = *self.u.types().get(t) else { return t };
        let Type::Bounds(lo, hi) = *self.u.types().get(result) else { return t };
        let params = params.clone();
        let hi = self.u.intern_type(Type::Lambda { binder, params: params.clone(), result: hi });
        let lo = match lo == crate::NOTHING {
            true => lo,
            false => self.u.intern_type(Type::Lambda { binder, params, result: lo }),
        };
        self.u.intern_type(Type::Bounds(lo, hi))
    }

    /// `t` with the parameters of binding `binding` replaced by `refs`: its parameter
    /// references, and the symbols of a method's type parameters of the same names.
    fn renamed(&mut self, binding: Binding, t: TypeId, refs: &[TypeId]) -> TypeId {
        let names: Vec<String> = binding.1.iter().map(|(n, _)| n.clone()).collect();
        let t = self.u.subst(t, binding.0, &names, refs);
        self.u.map_type(t, &mut |u, x| match u.types().get(x) {
            Type::Sym(_, s) if u.symbol(*s).kind == Kind::TypeParam => {
                let method = u.symbol(*s).owner.is_some_and(|o| u.symbol(o).kind == Kind::Def);
                let i = names.iter().position(|n| *n == u.symbol(*s).name)?;
                method.then(|| refs[i])
            }
            _ => None,
        })
    }
}
