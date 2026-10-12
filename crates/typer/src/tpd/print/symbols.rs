//! Symbols as `scripts/typer-oracle` prints them: full names, and methods' erased signatures.

use crate::signatures::params_sig;
use syntax::ast::print::quote;
use tasty::{Kind, SymbolId, Universe};

/// dotty's `fullName`: owners joined by `.`, each term owner between `s` and its enclosing class
/// as a `_$` before the name, the root and the empty package left out; the module class of `<:<`
/// as dotty gives it, `$:$$`.
pub fn full_name(u: &Universe, s: SymbolId) -> String {
    let name = &u.symbol(s).name;
    let unique = name.len() > 2
        && name.starts_with('$')
        && name.ends_with('$')
        && name[1..name.len() - 1].bytes().all(|b| b.is_ascii_digit());
    let name = match name.as_str() {
        "<:<$" if u.symbol(s).kind == Kind::Class => "$:$$",
        _ if unique => &name[..name.len() - 1],
        _ => name.as_str(),
    };
    if s == u.root() {
        return "<empty>".into();
    }
    let mut filler = String::new();
    let mut encl = u.symbol(s).owner;
    while let Some(e) = encl.filter(|&e| !matches!(u.symbol(e).kind, Kind::Class | Kind::Package)) {
        encl = u.symbol(e).owner;
        filler.push_str("_$");
    }
    match encl {
        Some(e) if e != u.root() => format!("{}.{filler}{name}", full_name(u, e)),
        _ => format!("{filler}{name}"),
    }
}

/// `s` quoted, a method with its signature, as in `"scala.Predef$.println#(java.lang.Object)"`.
pub fn sym(u: &mut Universe, s: SymbolId) -> String {
    let mut text = full_name(u, s);
    if u.symbol(s).kind == Kind::Def && u.symbol(s).name != "<init>" {
        text.push_str(&format!("#({})", params_sig(u, s).join(",")));
    }
    quote(&text.encode_utf16().collect::<Vec<_>>())
}
