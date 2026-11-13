// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The names pickled for symbols: module classes as `OBJECTCLASS` names, packages as qualified
//! names, default getters and unique names as derived names.

use super::TreePickler;
use names::{name_kinds, nme};
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
const UNIQUE_SEPARATORS: [&str; 8] =
    ["evidence$", "contextual$", "$proxy", "$given", "ev$", "ex$", "_$", "$"];

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
    if let Some(method) = digits.strip_suffix(name_kinds::DEFAULT_GETTER)
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

/// The name of a lambda's or method type's parameter `name`, a wildcard's `_$N` a unique name
/// (dotty's `WildcardParamName`).
pub fn param_name(name: &str) -> Name {
    match name.strip_prefix("_$").is_some_and(|n| n.parse::<u32>().is_ok()) {
        true => derived_name(name).unwrap_or_else(|| Name::utf8(name)),
        false => Name::utf8(name),
    }
}

/// The full name of package `p` (dotty's `fullName`), the root package's `<empty>`.
pub fn package_name(u: &Universe, p: SymbolId) -> Name {
    let name = if p == u.root() { nme::EMPTY_PACKAGE } else { u.symbol(p).name.as_str() };
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

/// The name of `s` and its owners up to its package as `ExpandPrefix` names, an object's
/// without its `$` and a `_$` for each term between it and its enclosing class (dotty's
/// `fullNameSeparated(ExpandPrefixName)`, as `Universe::expanded_prefix` spells it).
pub fn expanded_prefix_name(u: &Universe, s: SymbolId) -> Name {
    let sym = u.symbol(s);
    let mut name = tasty::module_name(&sym.name).unwrap_or(&sym.name).to_owned();
    let mut owner = sym.owner;
    while let Some(o) = owner.filter(|&o| u.symbol(o).kind.is_term()) {
        name.insert_str(0, "_$");
        owner = u.symbol(o).owner;
    }
    let last = Box::new(Name::utf8(&name));
    match owner {
        Some(o) if u.symbol(o).kind == Kind::Package => match u.full_name(o).as_str() {
            "" | nme::EMPTY_PACKAGE => *last,
            package => {
                let mut parts = package.split('.').map(Name::utf8);
                let first = parts.next().expect("a package's name");
                let package = parts.fold(first, |q, p| Name::ExpandPrefix(Box::new(q), Box::new(p)));
                Name::ExpandPrefix(Box::new(package), last)
            }
        },
        Some(o) => Name::ExpandPrefix(Box::new(expanded_prefix_name(u, o)), last),
        None => *last,
    }
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
            true => self.name(&Name::utf8(nme::ROOT_PKG)),
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

#[cfg(test)]
mod tests {
    use super::{
        derived_name, expanded_prefix_name, local_name, package_name, param_name, qualified, sym_name, utf8,
    };
    use tasty::tags::OBJECT;
    use tasty::write::Name;
    use tasty::{Flags, Kind, Symbol, SymbolId, Universe};

    fn define(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId) -> SymbolId {
        let flags = Flags::default();
        let (target, tree, decls) = (None, None, Vec::new());
        u.define(Symbol {
            name: name.into(),
            kind,
            owner: Some(owner),
            flags,
            target,
            is_extern: false,
            derived: false,
            decls,
            tree,
        })
    }

    fn unique(under: &str, sep: &str, num: u32) -> Name {
        Name::Unique { under: utf8(under), sep: sep.into(), num }
    }

    #[test]
    fn derived_names_as_dotty_s_name_kinds() {
        let dotty = [
            ("f$default$2", Some(Name::DefaultGetter(utf8("f"), 1))),
            ("x$1", Some(unique("x", "$", 1))),
            ("evidence$1", Some(unique("", "evidence$", 1))),
            ("contextual$2", Some(unique("", "contextual$", 2))),
            ("x$proxy3", Some(unique("x", "$proxy", 3))),
            ("x$given1", Some(unique("x", "$given", 1))),
            ("ev$4", Some(unique("", "ev$", 4))),
            ("_$5", Some(unique("", "_$", 5))),
            ("$1$", Some(unique("", "$", 1))),
            ("x$1$2", Some(unique("x$1", "$", 2))),
            (
                "a$b$3$4",
                Some(Name::Unique { under: Box::new(unique("a$b", "$", 3)), sep: "$".into(), num: 4 }),
            ),
            ("plain", None),
            ("x$01", None),
            ("x1", None),
        ];
        for (name, expected) in dotty {
            assert_eq!(derived_name(name), expected, "{name}");
        }
        assert_eq!(param_name("_$1"), unique("", "_$", 1));
        assert_eq!(param_name("_$x"), Name::utf8("_$x"));
        assert_eq!(param_name("x$1"), Name::utf8("x$1"));
    }

    #[test]
    fn names_of_packages_objects_and_locals() {
        let mut u = Universe::empty();
        let root = u.root();
        let p = define(&mut u, "p", Kind::Package, root);
        let q = define(&mut u, "q", Kind::Package, p);
        let qualified_pq = Name::Qualified(utf8("p"), utf8("q"));
        assert_eq!(package_name(&u, q), qualified_pq);
        assert_eq!(package_name(&u, root), Name::utf8("<empty>"));
        assert_eq!(qualified("p.q"), qualified_pq);

        let c = define(&mut u, "C", Kind::Class, q);
        let o = define(&mut u, "O$", Kind::Class, q);
        u.set_flags(o, Flags::default().with(OBJECT));
        let plain = define(&mut u, "D$", Kind::Class, q);
        assert_eq!(sym_name(&u, o), Name::ObjectClass(utf8("O")));
        assert_eq!(sym_name(&u, plain), Name::utf8("D$"));

        let m = define(&mut u, "m", Kind::Def, c);
        let local = define(&mut u, "L", Kind::Class, m);
        let in_object = define(&mut u, "I", Kind::Class, o);
        let prefixed = |outer: Name, last: &str| Name::ExpandPrefix(Box::new(outer), utf8(last));
        let c_name = prefixed(Name::ExpandPrefix(utf8("p"), utf8("q")), "C");
        assert_eq!(expanded_prefix_name(&u, c), c_name);
        assert_eq!(expanded_prefix_name(&u, local), prefixed(c_name, "_$L"));
        let o_name = prefixed(Name::ExpandPrefix(utf8("p"), utf8("q")), "O");
        assert_eq!(expanded_prefix_name(&u, in_object), prefixed(o_name, "I"));

        let getter = define(&mut u, "m$default$1", Kind::Def, c);
        let synthetic = define(&mut u, "x$1", Kind::Param, m);
        let fresh = define(&mut u, "x$1", Kind::Val, m);
        assert_eq!(local_name(&u, getter), Name::DefaultGetter(utf8("m"), 0));
        assert_eq!(local_name(&u, synthetic), Name::utf8("x$1"));
        assert_eq!(local_name(&u, fresh), unique("x", "$", 1));
    }
}
