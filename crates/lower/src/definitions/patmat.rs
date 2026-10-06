use super::{member, package, symbol, type_ref};
use tasty::{Flags, Kind, SymbolId, Type, Universe};

/// What compiled pattern matches refer to.
#[derive(Clone, Copy)]
pub struct Patmat {
    pub unit: SymbolId,
    pub int: SymbolId,
    /// `Any.==`.
    pub any_eq: SymbolId,
    /// `Boolean.&&`.
    pub and: SymbolId,
    pub match_error: SymbolId,
    pub match_error_ctor: SymbolId,
    /// dotty's `<special-ops>.throw`.
    pub throw: SymbolId,
}

impl Patmat {
    pub fn new(u: &mut Universe, any: SymbolId) -> Result<Patmat, String> {
        let scala = package(u, "scala")?;
        let java_lang = package(u, "java.lang")?;
        let boolean = member(u, scala, "Boolean", Kind::Class)?;
        let match_error = member(u, scala, "MatchError", Kind::Class)?;
        Ok(Patmat {
            unit: member(u, scala, "Unit", Kind::Class)?,
            int: member(u, scala, "Int", Kind::Class)?,
            any_eq: member(u, any, "==", Kind::Def)?,
            and: member(u, boolean, "&&", Kind::Def)?,
            match_error,
            match_error_ctor: member(u, match_error, "<init>", Kind::Def)?,
            throw: throw(u, java_lang)?,
        })
    }
}

/// `<special-ops>.throw(x: Throwable): Nothing`, defined if the typer has not.
fn throw(u: &mut Universe, java_lang: SymbolId) -> Result<SymbolId, String> {
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
