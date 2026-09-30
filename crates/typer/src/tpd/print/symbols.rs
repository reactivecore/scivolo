//! Symbols as `scripts/typer-oracle` prints them: full names, and methods' erased signatures.

use syntax::ast::print::quote;
use tasty::{Erased, Kind, SymbolId, Type, TypeId, Universe};

/// dotty's `fullName`: owners joined by `.`, each term owner between `s` and its enclosing class
/// as a `_$` before the name, the root and the empty package left out.
pub fn full_name(u: &Universe, s: SymbolId) -> String {
    let name = &u.symbol(s).name;
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

/// The name of an erased parameter type in a signature: `scala.Int`, `java.lang.String[]`.
fn sig_name(u: &mut Universe, t: TypeId, e: &Erased) -> String {
    match e {
        Erased::Prim(c) => format!("scala.{}", prim_name(*c)),
        Erased::Unit => "scala.runtime.BoxedUnit".into(),
        Erased::Nothing => "scala.runtime.Nothing$".into(),
        Erased::Null => "scala.runtime.Null$".into(),
        Erased::Array(elem) => format!("{}[]", sig_name(u, t, elem)),
        Erased::Class(binary) => match u.erased_class(t) {
            Some(c) if u.binary_name(c) == *binary => full_name(u, c),
            _ => binary.clone(),
        },
    }
}

fn prim_name(c: char) -> &'static str {
    match c {
        'z' => "Boolean",
        'c' => "Char",
        'b' => "Byte",
        's' => "Short",
        'i' => "Int",
        'j' => "Long",
        'f' => "Float",
        'd' => "Double",
        _ => "?",
    }
}

/// `s` quoted, a method with its signature (dotty's `paramsSig`): the number of type parameters
/// of each type parameter clause and the erased types of the value parameters, in order, as in
/// `"scala.Predef$.println#(java.lang.Object)"`.
pub fn sym(u: &mut Universe, s: SymbolId) -> String {
    let mut text = full_name(u, s);
    if u.symbol(s).kind == Kind::Def && u.symbol(s).name != "<init>" {
        let (mut erased, _) = u.erased_method(s);
        erased.reverse();
        let mut t = u.info(s);
        let mut parts = Vec::new();
        loop {
            match u.types().get(t).clone() {
                Type::Poly { params, result, .. } => {
                    parts.push(params.len().to_string());
                    t = result;
                }
                Type::Method { params, result, .. } => {
                    for (_, p) in params {
                        let Some(e) = erased.pop() else { break };
                        parts.push(sig_name(u, p, &e));
                    }
                    t = result;
                }
                _ => break,
            }
        }
        text.push_str(&format!("#({})", parts.join(",")));
    }
    quote(&text.encode_utf16().collect::<Vec<_>>())
}
