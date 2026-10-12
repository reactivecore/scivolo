//! The types a read tree may have: none the decoder could not decode or resolve, and no match
//! type, which dotty reduces.

use super::{Refusal, Unpickler};
use crate::tpd::TNodeId;
use tasty::{Kind, Type, TypeId, Universe};

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
        let mut refusal = None;
        self.u.map_type(t, &mut |u, part| {
            let found = match u.types().get(part).clone() {
                Type::Match(_) => Some("match type in a library inline body"),
                Type::Other(_) | Type::Local(..) | Type::PackageName(_) => {
                    Some("undecoded type in a library inline body")
                }
                Type::TypeRef(p, n) | Type::TermRef(p, n) => {
                    let is_type = matches!(u.types().get(part), Type::TypeRef(..));
                    match u.resolve(p, &n, is_type) {
                        Some(s) if is_match_alias(u, s) => Some("match type in a library inline body"),
                        Some(_) => None,
                        None => Some("unresolved reference in a library inline body"),
                    }
                }
                Type::Sym(_, s) if is_match_alias(u, s) => Some("match type in a library inline body"),
                _ => None,
            };
            refusal = refusal.or(found);
            None
        });
        refusal.map_or(Ok(()), Err)
    }
}

/// Whether `s` is a type whose alias or upper bound is a match type.
fn is_match_alias(u: &mut Universe, s: tasty::SymbolId) -> bool {
    if u.symbol(s).kind != Kind::Type {
        return false;
    }
    let info = u.info(s);
    match u.types().get(info).clone() {
        Type::Alias(a) | Type::Bounds(_, a) => is_match(u, a),
        Type::Lambda { result, .. } => is_match(u, result),
        _ => false,
    }
}

/// Whether `t` is a match type, also as the result of a type lambda.
fn is_match(u: &Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::Match(_) => true,
        Type::Lambda { result, .. } => is_match(u, result),
        _ => false,
    }
}
