// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The names pickled for symbols: module classes as `OBJECTCLASS` names, packages as qualified
//! names, default getters and unique names as derived names.

use super::TreePickler;
use tasty::tags::{OBJECT, PARAMsetter, TERMREFpkg};
use tasty::write::Name;
use tasty::{Kind, SymbolId, Universe};

/// The name of `s`.
pub fn sym_name(u: &Universe, s: SymbolId) -> Name {
    let sym = u.symbol(s);
    match sym.name.strip_suffix('$') {
        Some(n) if sym.kind == Kind::Class && sym.flags.has(OBJECT) => Name::ObjectClass(utf8(n)),
        _ => Name::utf8(&sym.name),
    }
}

/// The separators of dotty's unique name kinds, longest first.
const UNIQUE_SEPARATORS: [&str; 7] = ["evidence$", "contextual$", "$given", "ev$", "ex$", "_$", "$"];

/// The name of definition `s` of a quote: a default getter's or a unique name if it ends in
/// `$default$` or one of dotty's unique separators and a number, and if read from TASTy, its name
/// there is of such a kind; a synthetic parameter's `x$N` simple (dotty's `syntheticParamName`).
pub fn local_name(u: &Universe, s: SymbolId) -> Name {
    let sym = u.symbol(s);
    let param = sym.kind == Kind::Param || sym.flags.has(PARAMsetter);
    let synthetic = sym.name.strip_prefix("x$").is_some_and(|n| n.parse::<u32>().is_ok());
    if sym.tree.is_some() && !sym.derived || param && synthetic && !sym.derived {
        return sym_name(u, s);
    }
    derived_name(&sym.name).unwrap_or_else(|| sym_name(u, s))
}

/// Name `name` as a default getter's or a unique name, the name it is derived from also one
/// unless that is dotty's synthetic parameter `x$N`, a simple name; `$N$` the unique name of
/// nothing (dotty's `UniqueName.mkString`).
fn derived_name(name: &str) -> Option<Name> {
    if let Some(n) = name.strip_prefix('$').and_then(|n| n.strip_suffix('$'))
        && let Ok(num) = n.parse::<u32>()
        && !n.starts_with(['0', '+'])
    {
        return Some(Name::Unique { under: utf8(""), sep: "$".to_string(), num });
    }
    let digits = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let num: u32 = name[digits.len()..].parse().ok().filter(|_| !name[digits.len()..].starts_with('0'))?;
    if let Some(method) = digits.strip_suffix("$default$")
        && num > 0
    {
        return Some(Name::DefaultGetter(utf8(method), num - 1));
    }
    let sep = UNIQUE_SEPARATORS.iter().find(|&&sep| digits.ends_with(sep))?;
    let under = &digits[..digits.len() - sep.len()];
    let synthetic = under.strip_prefix("x$").is_some_and(|n| n.parse::<u32>().is_ok());
    let inner = derived_name(under).filter(|_| !synthetic);
    let under = inner.map_or_else(|| utf8(under), Box::new);
    Some(Name::Unique { under, sep: sep.to_string(), num })
}

/// The full name of package `p` (dotty's `fullName`), the root package's `<empty>`.
pub fn package_name(u: &Universe, p: SymbolId) -> Name {
    let name = if p == u.root() { "<empty>" } else { u.symbol(p).name.as_str() };
    match u.symbol(p).owner.filter(|&o| o != u.root()) {
        Some(o) => Name::Qualified(Box::new(package_name(u, o)), utf8(name)),
        None => Name::utf8(name),
    }
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
        if self.keying {
            let text = format!("{name:?}");
            self.nat(text.len() as u64);
            return self.buf.buf.write_bytes(text.as_bytes());
        }
        let r = self.pickler.names.index(name);
        self.nat(r.0 as u64);
    }

    pub fn package_name(&mut self, p: SymbolId) {
        let name = package_name(self.u, p);
        self.name(&name);
    }

    /// `TERMREFpkg` of package `p`, the root package's as `_root_`.
    pub fn package_ref(&mut self, p: SymbolId) {
        self.byte(TERMREFpkg);
        match p == self.u.root() {
            true => self.name(&Name::utf8("_root_")),
            false => self.package_name(p),
        }
    }

    /// The name of `s` in a reference: its local name if defined in the quote, else a default
    /// getter's if it is one.
    pub fn ref_name(&self, s: SymbolId) -> Name {
        if let Some(name) = self.accessor_name(s) {
            return name;
        }
        match (self.is_defined_here(s), derived_name(&self.u.symbol(s).name)) {
            (true, _) => local_name(self.u, s),
            (false, Some(getter @ Name::DefaultGetter(..))) => getter,
            (false, _) => sym_name(self.u, s),
        }
    }
}
