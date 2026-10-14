use super::{member, package, symbol, type_ref};
use tasty::{Flags, Kind, SymbolId, Type, Universe};

/// What compiled pattern matches refer to beyond `Any`'s, `Object`'s and `Boolean`'s members.
#[derive(Clone, Copy)]
pub struct Patmat {
    pub int: SymbolId,
    pub match_error: SymbolId,
    pub match_error_ctor: SymbolId,
}

impl Patmat {
    pub fn new(u: &mut Universe) -> Result<Patmat, String> {
        let scala = package(u, "scala")?;
        let match_error = member(u, scala, "MatchError", Kind::Class)?;
        Ok(Patmat {
            int: member(u, scala, "Int", Kind::Class)?,
            match_error,
            match_error_ctor: member(u, match_error, "<init>", Kind::Def)?,
        })
    }
}

/// `<special-ops>.throw(x: Throwable): Nothing`, defined if the typer has not.
pub fn throw(u: &mut Universe) -> Result<SymbolId, String> {
    let java_lang = package(u, "java.lang")?;
    let ops = u.enter_package("<special-ops>");
    if let Ok(found) = member(u, ops, "throw", Kind::Def) {
        return Ok(found);
    }
    let throwable = member(u, java_lang, "Throwable", Kind::Class)?;
    let throwable = type_ref(u, throwable);
    let nothing = u.builtins().nothing;
    let result = type_ref(u, nothing);
    let binder = u.fresh_binder();
    let info = u.intern_type(Type::Method {
        binder,
        params: vec![("x".into(), throwable)],
        result,
        implicit: false,
        contextual: false,
    });
    let throw = u.define(symbol("throw", Kind::Def, ops, Flags::default()));
    u.set_info(throw, info);
    Ok(throw)
}
