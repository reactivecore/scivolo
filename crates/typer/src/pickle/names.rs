// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//! The names pickled for symbols: module classes as `OBJECTCLASS` names, packages and signature
//! types as qualified names, methods with their signatures.

use super::TreePickler;
use crate::signatures::{params_sig, result_sig};
use tasty::tags::OBJECT;
use tasty::write::{Name, ParamSig};
use tasty::{Kind, SymbolId, Universe};

/// The name of `s`.
pub fn sym_name(u: &Universe, s: SymbolId) -> Name {
    let sym = u.symbol(s);
    match sym.name.strip_suffix('$') {
        Some(n) if sym.kind == Kind::Class && sym.flags.has(OBJECT) => Name::ObjectClass(utf8(n)),
        _ => Name::utf8(&sym.name),
    }
}

/// The full name of package `p` (dotty's `fullName`), the root package's `<empty>`.
fn package_name(u: &Universe, p: SymbolId) -> Name {
    let name = if p == u.root() { "<empty>" } else { u.symbol(p).name.as_str() };
    match u.symbol(p).owner.filter(|&o| o != u.root()) {
        Some(o) => Name::Qualified(Box::new(package_name(u, o)), utf8(name)),
        None => Name::utf8(name),
    }
}

/// The name of a class in a signature, `full` its full name: qualified, a module class's as an
/// `OBJECTCLASS` name, each array dimension a `[]` on the last part.
fn sig_type_name(full: &str) -> Name {
    let base = full.trim_end_matches("[]");
    let arrays = &full[base.len()..];
    let (module, base) = match base.strip_suffix('$') {
        Some(b) if !b.is_empty() && !b.ends_with('.') => (true, b),
        _ => (false, base),
    };
    let name = qualified(&format!("{base}{arrays}"));
    if module { Name::ObjectClass(Box::new(name)) } else { name }
}

/// The qualified name of dotted path `path`.
pub fn qualified(path: &str) -> Name {
    let mut parts = path.split('.').map(Name::utf8);
    let first = parts.next().unwrap_or_else(|| Name::utf8(""));
    parts.fold(first, |q, p| Name::Qualified(Box::new(q), Box::new(p)))
}

fn utf8(s: &str) -> Box<Name> {
    Box::new(Name::utf8(s))
}

impl TreePickler<'_> {
    pub fn name(&mut self, name: &Name) {
        let r = self.pickler.names.index(name);
        self.nat(r.0 as u64);
    }

    pub fn package_name(&mut self, p: SymbolId) {
        let name = package_name(self.u, p);
        self.name(&name);
    }

    /// The name of `s`, signed if `signed` and `s` is a method (dotty's `pickleNameAndSig`).
    pub fn signed_name(&mut self, s: SymbolId, signed: bool) {
        let orig = sym_name(self.u, s);
        let result = if signed { result_sig(self.u, s) } else { None };
        let Some(result) = result else { return self.name(&orig) };
        let params = params_sig(self.u, s)
            .iter()
            .map(|p| match p.parse() {
                Ok(n) => ParamSig::TypeParams(n),
                Err(_) => ParamSig::Term(sig_type_name(p)),
            })
            .collect();
        let target = match &self.u.symbol(s).target {
            Some(t) => Name::utf8(t),
            None => orig.clone(),
        };
        let result = Box::new(sig_type_name(&result));
        self.name(&Name::Signed { orig: Box::new(orig), target: Box::new(target), result, params });
    }
}
