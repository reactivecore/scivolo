// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//! The names pickled for symbols: module classes as `OBJECTCLASS` names, packages and signature
//! types as qualified names, methods with their signatures.

use super::{Refusal, TreePickler};
use crate::signatures::{params_sig, result_sig};
use tasty::tags::{OBJECT, PARAMsetter, TERMREFpkg};
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
/// unless that is dotty's synthetic parameter `x$N`, a simple name.
fn derived_name(name: &str) -> Option<Name> {
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
fn package_name(u: &Universe, p: SymbolId) -> Name {
    let name = if p == u.root() { "<empty>" } else { u.symbol(p).name.as_str() };
    match u.symbol(p).owner.filter(|&o| o != u.root()) {
        Some(o) => Name::Qualified(Box::new(package_name(u, o)), utf8(name)),
        None => Name::utf8(name),
    }
}

/// The name of a class in a signature, `full` its full name: qualified, a module class's as an
/// `OBJECTCLASS` name, each array dimension a `[]` on the last part.
fn sig_type_name(u: &mut Universe, full: &str) -> Result<Name, Refusal> {
    let base = full.trim_end_matches("[]");
    let arrays = &full[base.len()..];
    let (module, base) = match base.strip_suffix('$') {
        Some(b) if is_module_class(u, base)? => (true, b),
        _ => (false, base),
    };
    let path = format!("{base}{arrays}");
    let parts: Vec<&str> = path.split('.').collect();
    let mut name: Option<Name> = None;
    for (i, part) in parts.iter().enumerate() {
        let object = i + 1 < parts.len() && part.len() > 1 && part.ends_with('$');
        let part = Name::utf8(if object { &part[..part.len() - 1] } else { part });
        let qualified = match name {
            Some(q) => Name::Qualified(Box::new(q), Box::new(part)),
            None => part,
        };
        name = Some(if object { Name::ObjectClass(Box::new(qualified)) } else { qualified });
    }
    let name = name.expect("a name");
    Ok(if module { Name::ObjectClass(Box::new(name)) } else { name })
}

/// Whether the class of full name `full`, which ends in `$`, is a module class.
fn is_module_class(u: &mut Universe, full: &str) -> Result<bool, Refusal> {
    let (pkg, name) = full.rsplit_once('.').unwrap_or(("", full));
    let p = u.package(pkg).ok_or("a signature naming a class outside a package")?;
    let decls = u.decl(p, name).map_err(|_| "a signature naming an unknown class")?;
    let class = decls.into_iter().find(|&s| u.symbol(s).kind == Kind::Class);
    let class = class.ok_or("a signature naming an unknown class")?;
    Ok(u.symbol(class).flags.has(OBJECT))
}

/// The name of class `c` in a signature (dotty's `fullNameSeparated(QualifiedName)`): a `_$` for
/// each term between it and its enclosing class, a module class's as an `OBJECTCLASS` name.
fn class_sig_name(u: &Universe, c: SymbolId) -> Name {
    let sym = u.symbol(c);
    let module = sym.kind == Kind::Class && sym.flags.has(OBJECT);
    let mut name = match sym.name.strip_suffix('$') {
        Some(n) if module => n.to_owned(),
        _ => sym.name.clone(),
    };
    let mut encl = sym.owner;
    while let Some(e) = encl.filter(|&e| !matches!(u.symbol(e).kind, Kind::Class | Kind::Package)) {
        name.insert_str(0, "_$");
        encl = u.symbol(e).owner;
    }
    let name = match encl.filter(|&e| e != u.root()) {
        Some(e) if u.symbol(e).kind == Kind::Package => {
            Name::Qualified(Box::new(package_name(u, e)), utf8(&name))
        }
        Some(e) => Name::Qualified(Box::new(class_sig_name(u, e)), utf8(&name)),
        None => Name::utf8(&name),
    };
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
        match (self.is_defined_here(s), derived_name(&self.u.symbol(s).name)) {
            (true, _) => local_name(self.u, s),
            (false, Some(getter @ Name::DefaultGetter(..))) => getter,
            (false, _) => sym_name(self.u, s),
        }
    }

    /// The name of `s`, signed if `signed` and `s` is a method (dotty's `pickleNameAndSig`).
    pub fn signed_name(&mut self, s: SymbolId, signed: bool) -> Result<(), Refusal> {
        let orig = self.ref_name(s);
        let result = if signed { result_sig(self.u, s) } else { None };
        let Some(result) = result else {
            self.name(&orig);
            return Ok(());
        };
        let params = params_sig(self.u, s)
            .iter()
            .map(|p| match p.parse() {
                Ok(n) => Ok(ParamSig::TypeParams(n)),
                Err(_) => sig_type_name(self.u, p).map(ParamSig::Term),
            })
            .collect::<Result<_, _>>()?;
        let target = match &self.u.symbol(s).target {
            Some(t) => Name::utf8(t),
            None => orig.clone(),
        };
        let class = self.u.symbol(s).owner.filter(|_| self.u.is_constructor(s));
        let result = match class.filter(|&c| self.u.underlying(c).is_none()) {
            Some(class) => Box::new(class_sig_name(self.u, class)),
            None => Box::new(sig_type_name(self.u, &result)?),
        };
        self.name(&Name::Signed { orig: Box::new(orig), target: Box::new(target), result, params });
        Ok(())
    }
}
