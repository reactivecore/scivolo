//! Parameter and result types an application leaves to dotty: by-name, dependent.

use super::Typer;
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether parameter type `p` is by-name or refers to a parameter.
    pub fn is_special_param(&mut self, p: TypeId) -> bool {
        let mut special = false;
        self.u.map_type(p, &mut |u, part| {
            special |= matches!(u.types().get(part), Type::ByName(_) | Type::Param(..));
            None
        });
        special
    }

    /// Whether type `t` refers to a value parameter of the method type of binder `binder`, or of
    /// method `method`.
    pub fn mentions_params(&mut self, t: TypeId, binder: Binder, method: Option<SymbolId>) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            match *u.types().get(part) {
                Type::Param(b, _) if b == binder => found = true,
                Type::Sym(_, s) if u.symbol(s).kind == Kind::Param && method.is_some() => {
                    found |= u.symbol(s).owner == method;
                }
                _ => {}
            }
            None
        });
        found
    }
}
