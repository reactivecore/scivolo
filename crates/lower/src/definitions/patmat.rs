use super::{member, symbol, type_ref};
use tasty::{Flags, Kind, SymbolId, Universe, nme};

/// What compiled pattern matches refer to beyond `Any`'s, `Object`'s and `Boolean`'s members.
#[derive(Clone, Copy)]
pub struct Patmat {
    pub int: SymbolId,
    pub match_error: SymbolId,
    pub match_error_ctor: SymbolId,
}

impl Patmat {
    pub fn new(u: &mut Universe) -> Result<Patmat, String> {
        let scala = u.defn().scala;
        let match_error = member(u, scala, "MatchError", Kind::Class)?;
        Ok(Patmat {
            int: member(u, scala, "Int", Kind::Class)?,
            match_error,
            match_error_ctor: member(u, match_error, nme::CONSTRUCTOR, Kind::Def)?,
        })
    }
}

/// `<special-ops>.throw(x: Throwable): Nothing`, defined if the typer has not.
pub fn throw(u: &mut Universe) -> Result<SymbolId, String> {
    let ops = u.enter_package(nme::OPS_PACKAGE);
    if let Ok(found) = member(u, ops, "throw", Kind::Def) {
        return Ok(found);
    }
    let throwable = u.defn().throwable(u)?;
    let throwable = type_ref(u, throwable);
    let nothing = u.defn().nothing;
    let result = type_ref(u, nothing);
    let binder = u.fresh_binder();
    let params = [(u.intern_name("x"), throwable)];
    let info = u.intern_method(binder, &params, result, false, false);
    let throw = u.define(symbol("throw", Kind::Def, ops, Flags::default()));
    u.set_info(throw, info);
    Ok(throw)
}
