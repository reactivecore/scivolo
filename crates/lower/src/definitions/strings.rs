use super::{member, package};
use tasty::{Erased, Kind, SymbolId, Universe};

/// What string concatenation calls (nscplugin's `jlStringBuilder*` definitions).
pub struct Strings {
    pub string: SymbolId,
    /// `String.valueOf(Object)`, a method of the companion object.
    pub value_of: SymbolId,
    pub builder: SymbolId,
    /// `StringBuilder(capacity: Int)`.
    pub builder_ctor: SymbolId,
    /// The `append` methods of `StringBuilder` with one parameter.
    pub appends: Vec<SymbolId>,
    pub builder_to_string: SymbolId,
    pub string_buffer: SymbolId,
    pub char_sequence: SymbolId,
}

impl Strings {
    pub fn new(u: &mut Universe) -> Result<Strings, String> {
        let java_lang = package(u, "java.lang")?;
        let string = member(u, java_lang, "String", Kind::Class)?;
        let companion = member(u, java_lang, "String", Kind::Val)?;
        let companion = u.module_class(companion).ok_or("java.lang.String's object missing")?;
        let object = Erased::Class("java.lang.Object".into());
        let value_of = with_params(u, companion, "valueOf", |ps| ps == [object.clone()])
            .ok_or("java.lang.String.valueOf(Object) missing")?;
        let builder = member(u, java_lang, "StringBuilder", Kind::Class)?;
        let builder_ctor = with_params(u, builder, "<init>", |ps| ps == [Erased::Prim('i')])
            .ok_or("java.lang.StringBuilder(Int) missing")?;
        let appends = all_with_params(u, builder, "append", |ps| ps.len() == 1);
        let builder_to_string = with_params(u, builder, "toString", <[Erased]>::is_empty)
            .ok_or("java.lang.StringBuilder.toString missing")?;
        Ok(Strings {
            string,
            value_of,
            builder,
            builder_ctor,
            appends,
            builder_to_string,
            string_buffer: member(u, java_lang, "StringBuffer", Kind::Class)?,
            char_sequence: member(u, java_lang, "CharSequence", Kind::Class)?,
        })
    }
}

/// The first method `name` of `owner` whose erased parameters pass `ok`.
fn with_params(
    u: &mut Universe,
    owner: SymbolId,
    name: &str,
    ok: impl Fn(&[Erased]) -> bool,
) -> Option<SymbolId> {
    all_with_params(u, owner, name, ok).into_iter().next()
}

fn all_with_params(
    u: &mut Universe,
    owner: SymbolId,
    name: &str,
    ok: impl Fn(&[Erased]) -> bool,
) -> Vec<SymbolId> {
    let methods = u.decl(owner, name).unwrap_or_default();
    methods.into_iter().filter(|&m| u.symbol(m).kind == Kind::Def && ok(&u.erased_method(m).0)).collect()
}
