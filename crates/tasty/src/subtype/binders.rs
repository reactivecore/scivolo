//! Type lambdas and polymorphic method types: parameters renamed to a common binder, bounds
//! compared contravariantly, then the bodies.

use super::Cmp;
use crate::types::{Binder, Type, TypeId};
use crate::{FileId, Kind};

/// A binding type: its binder, its parameters' names and bounds, its body.
pub type Binding<'a> = (Binder, &'a [(String, TypeId)], TypeId);

impl Cmp<'_> {
    /// Whether binding type `a` is a subtype of `b`: as many parameters, each bound of `b`'s
    /// within `a`'s, `a`'s body a subtype of `b`'s.
    pub fn compare_binders(&mut self, a: Binding, b: Binding) -> bool {
        if a.1.len() != b.1.len() {
            return false;
        }
        let common = Binder(FileId(u32::MAX), self.depth);
        let refs: Vec<TypeId> =
            (0..a.1.len() as u32).map(|i| self.u.intern_type(Type::Param(common, i))).collect();
        let bounds_ok = a.1.iter().zip(b.1).all(|(&(_, bound_a), &(_, bound_b))| {
            let (bound_a, bound_b) = (self.renamed(a, bound_a, &refs), self.renamed(b, bound_b, &refs));
            self.recur(bound_b, bound_a)
        });
        let (body_a, body_b) = (self.renamed(a, a.2, &refs), self.renamed(b, b.2, &refs));
        bounds_ok && self.recur(body_a, body_b)
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
