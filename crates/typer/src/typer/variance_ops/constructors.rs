// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Variances.scala
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala
//! The variances of the parameters of type constructors.

use super::{Checked, VarianceOps};
use tasty::{Binder, SymbolId, Type, TypeId};

impl VarianceOps<'_> {
    /// Determines the variances of the parameters of type constructor `tycon`: an alias's
    /// declared ones, else +1 for a parameter occurring only covariantly or not at all, -1 for one
    /// occurring only contravariantly, 0 otherwise (dotty's `paramVarianceSign`). Of
    /// `type F[X] = X => Int` they are -1; of a class, its declared ones.
    pub fn tycon_variances(&mut self, tycon: TypeId) -> Vec<i8> {
        let alias = self.u.referenced(tycon).and_then(|s| self.u.alias_of(s));
        let Some((binder, params, result)) =
            alias.and_then(|a| self.u.lambda(a)).map(|l| (l.binder, l.params.to_vec(), l.result))
        else {
            return self.u.constructor_variances(tycon);
        };
        if let Some(declared) = self.u.declared_variances(binder) {
            return declared;
        }
        if let Some(known) = self.aliases.get(&binder) {
            return known.clone();
        }
        self.aliases.insert(binder, vec![1; params.len()]);
        let mut probe = vec![0; params.len()];
        let mut signs = Vec::with_capacity(params.len());
        for i in 0..params.len() {
            let sign = [1, -1].into_iter().find(|&v| {
                probe[i] = v;
                let found = self.misplaced(Checked::Lambda(binder, &probe), result, 1, 0).is_some();
                probe[i] = 0;
                !found
            });
            signs.push(sign.unwrap_or(0));
        }
        self.aliases.insert(binder, signs.clone());
        signs
    }

    /// The variances the bounds of type parameter `i` of polymorphic type `binder` in the type
    /// of `base` declare.
    pub fn poly_param_variances(&mut self, base: SymbolId, binder: Binder, i: u32) -> Vec<i8> {
        let mut info = self.u.info(base);
        loop {
            match *self.u.ty(info) {
                Type::Poly { binder: b, params, .. } if b == binder => {
                    let Some(&(_, bounds)) = self.u.params_of(params).get(i as usize) else {
                        return Vec::new();
                    };
                    let hi = self.u.bounds(bounds).map_or(bounds, |(_, hi)| hi);
                    let Some(lambda) = self.u.lambda(hi).map(|l| l.binder) else {
                        return Vec::new();
                    };
                    return self.u.declared_variances(lambda).unwrap_or_default();
                }
                Type::Poly { result, .. } | Type::Method { result, .. } => info = result,
                _ => return Vec::new(),
            }
        }
    }
}
