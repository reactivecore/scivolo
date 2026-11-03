//! `TypeLambda.paramVariances`.

use super::Fallible;
use crate::host::Host;
use tasty::TypeId;

impl Host {
    pub fn param_variances(&mut self, t: TypeId) -> Fallible<Vec<i32>> {
        self.u.lambda_variance_flags(t, &[]).ok_or("TypeLambda.paramVariances of another type")
    }
}
