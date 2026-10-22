// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Symbols as `scripts/typer-oracle` prints them: full names, and methods' erased signatures.

use crate::signatures::params_sig;
use syntax::ast::print::quote;
use tasty::{Kind, SymbolId, Universe};

/// dotty's `fullName`: owners joined by `.`, each term owner between `s` and its enclosing class
/// as a `_$` before the name, the root and the empty package left out; the module class of `<:<`
/// as dotty gives it, `$:$$`; a context function parameter `contextual$N` as an empty name; the
/// owners of evidence `evidence$N` with `$` for `<` and `>`.
pub fn full_name(u: &Universe, s: SymbolId) -> String {
    let name = &u.symbol(s).name;
    let digits = |n: &str| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
    let unique =
        name.len() > 2 && name.starts_with('$') && name.ends_with('$') && digits(&name[1..name.len() - 1]);
    let name = match name.as_str() {
        "<:<$" if u.symbol(s).kind == Kind::Class => "$:$$",
        n if n.strip_prefix("contextual$").is_some_and(digits) => "",
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
    let prefix = match encl {
        Some(e) if e != u.root() => format!("{}.", full_name(u, e)),
        _ => String::new(),
    };
    let prefix = match name.strip_prefix("evidence$").is_some_and(digits) {
        true => prefix.replace(['<', '>'], "$"),
        false => prefix,
    };
    format!("{prefix}{filler}{name}")
}

/// `s` quoted, a method with its signature, as in `"scala.Predef$.println#(java.lang.Object)"`.
pub fn sym(u: &mut Universe, s: SymbolId) -> String {
    let mut text = full_name(u, s);
    if u.symbol(s).kind == Kind::Def && u.symbol(s).name != "<init>" {
        text.push_str(&format!("#({})", params_sig(u, s).join(",")));
    }
    quote(&text.encode_utf16().collect::<Vec<_>>())
}
