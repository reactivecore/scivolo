// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Variances.scala

use super::Typer;
use super::variance_types::Checked;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The variances of the parameters of type constructor `tycon`: an alias's declared ones,
    /// else +1 for a parameter occurring only covariantly or not at all, -1 for one occurring only
    /// contravariantly, 0 otherwise (dotty's `paramVarianceSign`).
    pub fn tycon_variances(&mut self, tycon: TypeId) -> Vec<i8> {
        let alias = self.u.referenced(tycon).and_then(|s| self.u.alias_of(s));
        let Some(Type::Lambda { binder, params, result }) = alias.map(|a| self.u.types().get(a).clone())
        else {
            return self.u.constructor_variances(tycon);
        };
        if let Some(declared) = self.u.declared_variances(binder) {
            return declared;
        }
        if let Some(known) = self.alias_variances.get(&binder) {
            return known.clone();
        }
        self.alias_variances.insert(binder, vec![1; params.len()]);
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
        self.alias_variances.insert(binder, signs.clone());
        signs
    }
}
