// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! Type lambdas of a pickle (dotty's `TempPolyType`, `translateTempPoly`) and the types of its
//! type members (`ALIASsym`, abstract `TYPEsym`).

use super::super::encoded_names::decode;
use super::super::{SymbolId, Universe};
use super::format::ALIASsym;
use super::{Unpickler, flags as f};
use crate::types::{NameId, Type, TypeId};

/// The parameters of a type lambda named with their bounds, their declared variances, and its
/// result.
struct LambdaParts {
    bounds: Vec<(NameId, TypeId)>,
    variances: Vec<i8>,
    result: TypeId,
}

impl Unpickler {
    /// The type lambda of a `POLYtpe` in a type's position over type parameter entries `params`
    /// and result entry `res` (dotty's `HKTypeLambda.fromParams`): over bounds, the variances its
    /// parameters declare; a `POLYtpe` without parameters is refused.
    pub fn type_lambda(&mut self, u: &mut Universe, res: usize, params: &[usize]) -> Result<TypeId, String> {
        if params.is_empty() {
            return Err("a parameterless method type in a type".into());
        }
        let binder = u.fresh_binder();
        for (k, &p) in params.iter().enumerate() {
            let param = u.types.intern(Type::Param(binder, k as u32));
            self.params.insert(p, param);
        }
        let read = self.lambda_parts(u, res, params);
        params.iter().for_each(|p| _ = self.params.remove(p));
        let LambdaParts { bounds, variances, result } = read?;
        let lambda = u.intern_lambda(binder, &bounds, result);
        if u.bounds(result).is_some() {
            u.declare_lambda_variances(binder, variances);
        }
        Ok(lambda)
    }

    /// The parameters of a lambda with their bounds, their declared variances and its result.
    fn lambda_parts(
        &mut self,
        u: &mut Universe,
        res: usize,
        params: &[usize],
    ) -> Result<LambdaParts, String> {
        let mut bounds = Vec::new();
        let mut variances = Vec::new();
        for &p in params {
            let h = self.header(p)?;
            let b = self.ty(u, h.info)?;
            bounds.push((u.types.intern_name(&decode(&h.name)), b));
            variances.push(match h.flags {
                fl if fl & f::COVARIANT != 0 => 1,
                fl if fl & f::CONTRAVARIANT != 0 => -1,
                _ => 0,
            });
        }
        let result = self.ty(u, res)?;
        Ok(LambdaParts { bounds, variances, result })
    }

    /// Gives type member `s` of entry `i` its type: an alias its right-hand side, an abstract
    /// type its bounds (dotty's `LocalUnpickler.complete` for `ALIASsym` and `TYPEsym`).
    pub fn type_member_info(&mut self, u: &mut Universe, i: usize, s: SymbolId) -> Result<(), String> {
        let h = self.header(i)?;
        let info = self.ty(u, h.info)?;
        let info = match (h.tag, *u.ty(info)) {
            (ALIASsym, Type::Lambda { .. }) => info,
            (ALIASsym, _) => u.types.intern(Type::Alias(info)),
            (_, Type::Bounds(..) | Type::Lambda { .. }) => info,
            _ => return Err("an abstract type member without bounds".into()),
        };
        u.infos.insert(s, info);
        Ok(())
    }
}
