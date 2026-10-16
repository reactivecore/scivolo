//! Class types disjoint through the arguments of a common base class (dotty's
//! `existsCommonBaseTypeWithDisjointArguments` and `provablyDisjointTypeArgs`).

use super::super::Cmp;
use super::disjoint::Pending;
use crate::tags::{CASE, CASEaccessor};
use crate::types::{Type, TypeId};
use crate::{Kind, NOTHING, SymbolId};

impl Cmp<'_> {
    pub fn common_base_with_disjoint_args(
        &mut self,
        x: TypeId,
        y: TypeId,
        (c1, c2): (SymbolId, SymbolId),
        pending: &mut Pending,
    ) -> bool {
        if !self.args_match(x, c1) || !self.args_match(y, c2) {
            return false;
        }
        pending.insert((x, y));
        let lin2 = self.u.linearization(c2);
        let common: Vec<SymbolId> =
            self.u.linearization(c1).into_iter().filter(|b| lin2.contains(b)).collect();
        let mut result = false;
        for &base in &common {
            let ancestor = common.iter().any(|&o| o != base && self.u.derives_from(o, base));
            if !ancestor && self.base_with_disjoint_args(x, y, base, pending) {
                result = true;
                break;
            }
        }
        pending.remove(&(x, y));
        result
    }

    /// Whether `t` passes class `c` as many arguments as it takes.
    fn args_match(&mut self, t: TypeId, c: SymbolId) -> bool {
        let n = match self.u.types().get(t) {
            Type::Applied(_, args) => args.len(),
            _ => 0,
        };
        self.u.type_params(c).len() == n
    }

    fn base_with_disjoint_args(
        &mut self,
        x: TypeId,
        y: TypeId,
        base: SymbolId,
        pending: &mut Pending,
    ) -> bool {
        let params = self.u.type_params(base);
        if params.is_empty() {
            return false;
        }
        let (Some(b1), Some(b2)) = (self.u.base_type(x, base), self.u.base_type(y, base)) else {
            return false;
        };
        let (Type::Applied(_, args1), Type::Applied(_, args2)) =
            (self.u.types().get(b1).clone(), self.u.types().get(b2).clone())
        else {
            return false;
        };
        let variances = self.u.variances(base);
        for (i, (&a1, &a2)) in args1.iter().zip(&args2).enumerate() {
            let disjoint = match variances.get(i).copied().unwrap_or(0) {
                v if v > 0 => self.disjoint_in(a1, a2, pending) && self.param_is_field(base, params[i]),
                v if v < 0 => false,
                _ => {
                    self.disjoint_in(a1, a2, pending)
                        && (self.param_is_field(base, params[i])
                            || self.cannot_be_nothing(a1)
                            || self.cannot_be_nothing(a2))
                }
            };
            if disjoint {
                return true;
            }
        }
        false
    }

    /// Whether type parameter `p` of class `c` is the type of one of its case class fields, or
    /// `c` is a pair class (dotty's `typeparamCorrespondsToField`).
    fn param_is_field(&mut self, c: SymbolId, p: SymbolId) -> bool {
        if self.u.full_name(c) == "scala.*:" {
            return true;
        }
        if !self.u.symbol(c).flags.has(CASE) {
            return false;
        }
        let fields: Vec<SymbolId> = self
            .u
            .symbol(c)
            .decls
            .iter()
            .copied()
            .filter(|&d| self.u.symbol(d).kind.is_term() && self.u.symbol(d).flags.has(CASEaccessor))
            .collect();
        fields.into_iter().any(|f| {
            let info = self.u.info(f);
            matches!(*self.u.types().get(info), Type::Sym(_, s) if s == p)
        })
    }

    /// Whether `t`'s lower bound is not `Nothing`.
    fn cannot_be_nothing(&mut self, t: TypeId) -> bool {
        if let Some(s) = self.named(t).and_then(|n| n.sym)
            && self.u.symbol(s).kind == Kind::TypeParam
        {
            let info = self.u.info(s);
            return self.cannot_be_nothing(info);
        }
        let lo = match self.arg_bounds(t) {
            Some((lo, _)) => lo,
            None => t,
        };
        !self.recur_frozen(lo, NOTHING)
    }
}
