//! The types a read tree may have: none the decoder could not decode or resolve.

use super::{Refusal, Unpickler};
use crate::tpd::TNodeId;
use tasty::{NameId, Type, TypeId, Universe};

impl Unpickler<'_> {
    /// Refuses tree `t` if its type is not admitted.
    pub fn check_type(&mut self, t: TNodeId) -> Result<(), Refusal> {
        match self.tpd.get(t).tpe {
            Some(tpe) => self.check(tpe),
            None => Ok(()),
        }
    }

    /// Refuses type `t` if it is not admitted.
    pub fn check(&mut self, t: TypeId) -> Result<(), Refusal> {
        refusal(self.u, t, &[]).map_or(Ok(()), Err)
    }
}

/// Why type `t` is not admitted, the parameters of the type lambdas around it named by `bound`.
fn refusal(u: &mut Universe, t: TypeId, bound: &[(tasty::FileId, NameId)]) -> Option<Refusal> {
    let mut found = None;
    u.map_type(t, &mut |u, part| {
        let refused = match *u.ty(part) {
            Type::Lambda { binder, params, result } => {
                let mut inner = bound.to_vec();
                inner.extend(u.params_of(params).iter().map(|&(n, _)| (binder.0, n)));
                let in_params = (0..u.params_of(params).len()).find_map(|i| {
                    let p = u.params_of(params)[i].1;
                    refusal(u, p, &inner)
                });
                found = found.or(in_params).or_else(|| refusal(u, result, &inner));
                return Some(part);
            }
            Type::Local(b, n) if bound.iter().any(|(f, m)| *f == b.0 && *m == n) => None,
            Type::Other(_) | Type::Local(..) | Type::PackageName(_) => {
                Some("undecoded type in a library inline body")
            }
            Type::TypeRef(p, n) | Type::TermRef(p, n) => {
                let is_type = u.type_ref(part).is_some();
                match u.resolve(p, n, is_type) {
                    Some(_) => None,
                    None if is_type && u.refinement_named(p, n).is_some() => None,
                    None if u.param_ref(p).is_some() => None,
                    None if !is_type && u.name(n) == "apply" && u.poly_function_apply_of(p).is_some() => None,
                    None => Some("unresolved reference in a library inline body"),
                }
            }
            _ => None,
        };
        found = found.or(refused);
        None
    });
    found
}
