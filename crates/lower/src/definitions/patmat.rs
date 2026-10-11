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
    /// `Any.isInstanceOf`, and `Any.$isInstanceOf` that pattern matches test with.
    pub is_instance_of: SymbolId,
    pub type_test: SymbolId,
    /// `Object.ne`.
    pub ne: SymbolId,
    /// `Object.eq`.
    pub eq: SymbolId,
}

impl Patmat {
    pub fn new(u: &mut Universe, any: SymbolId, object: SymbolId) -> Result<Patmat, String> {
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
            is_instance_of: member(u, any, "isInstanceOf", Kind::Def)?,
            type_test: member(u, any, "$isInstanceOf", Kind::Def)?,
            ne: member(u, object, "ne", Kind::Def)?,
            eq: member(u, object, "eq", Kind::Def)?,
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
