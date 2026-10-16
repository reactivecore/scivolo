//! The types a read tree may have: none the decoder could not decode or resolve.

use super::{Refusal, Unpickler};
use crate::tpd::TNodeId;
use tasty::Type;

impl Unpickler<'_> {
    /// Refuses tree `t` if its type is not admitted.
    pub fn check_type(&mut self, t: TNodeId) -> Result<(), Refusal> {
        match self.tpd.get(t).tpe {
            Some(tpe) => self.check(tpe),
            None => Ok(()),
        }
    }

    /// Refuses type `t` if it is not admitted.
    pub fn check(&mut self, t: tasty::TypeId) -> Result<(), Refusal> {
        let mut refusal = None;
        self.u.map_type(t, &mut |u, part| {
            let found = match u.types().get(part).clone() {
                Type::Other(_) | Type::Local(..) | Type::PackageName(_) => {
                    Some("undecoded type in a library inline body")
                }
                Type::TypeRef(p, n) | Type::TermRef(p, n) => {
                    let is_type = matches!(u.types().get(part), Type::TypeRef(..));
                    match u.resolve(p, &n, is_type) {
                        Some(_) => None,
                        None => Some("unresolved reference in a library inline body"),
                    }
                }
                _ => None,
            };
            refusal = refusal.or(found);
            None
        });
        refusal.map_or(Ok(()), Err)
    }
}
