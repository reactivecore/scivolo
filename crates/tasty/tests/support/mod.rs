//! The universe of `fixtures/subtype/Prelude.scala` and a parser of the fixtures' types.
#![allow(dead_code)]

use std::path::Path;
use tasty::{ANY, Flags, Kind, NOTHING, Symbol, SymbolId, Type, TypeId, Universe};

pub fn universe() -> Option<Universe> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fetched = [
        "scala-library-3.9.0.jar",
        "nativelib_native0.5_3-0.5.12.jar",
        "clib_native0.5_3-0.5.12.jar",
        "posixlib_native0.5_3-0.5.12.jar",
    ]
    .map(|j| root.join("ref/libs").join(j));
    if let Some(missing) = fetched.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    let mut jars = fetched.to_vec();
    jars.push(root.join("libs/javalib-tasty-0.5.12.jar"));
    jars.push(root.join("crates/tasty/tests/fixtures/subtype-prelude.jar"));
    Some(Universe::open(&jars).unwrap())
}

/// Parses the fixtures' types: qualified names, `[…]` arguments, `?` wildcards with bounds,
/// `p.type` singletons, integer and string literals, and `|`/`&` between them.
struct Parser<'a> {
    s: &'a str,
}

impl Parser<'_> {
    fn eat(&mut self, token: &str) -> bool {
        let found = self.s.starts_with(token);
        if found {
            self.s = &self.s[token.len()..];
        }
        found
    }

    fn infix(&mut self, u: &mut Universe) -> TypeId {
        let mut t = self.simple(u);
        loop {
            if self.eat(" | ") {
                let r = self.simple(u);
                t = u.intern_type(Type::Or(t, r, false));
            } else if self.eat(" & ") {
                let r = self.simple(u);
                t = u.intern_type(Type::And(t, r));
            } else {
                return t;
            }
        }
    }

    fn simple(&mut self, u: &mut Universe) -> TypeId {
        if self.eat("?") {
            let (mut lo, mut hi) = (NOTHING, ANY);
            if self.eat(" <: ") {
                hi = self.simple(u);
            } else if self.eat(" >: ") {
                lo = self.simple(u);
            }
            return u.intern_type(Type::Bounds(lo, hi));
        }
        if let Some(rest) = self.s.strip_prefix('"') {
            let (text, rest) = rest.split_once('"').unwrap();
            self.s = rest;
            return u.str_type(text);
        }
        let end = self.s.find(['[', ',', ']', ' ']).unwrap_or(self.s.len());
        let (path, rest) = self.s.split_at(end);
        self.s = rest;
        if let Ok(n) = path.parse::<i64>() {
            return u.intern_type(Type::Constant(tasty::tags::INTconst, n));
        }
        let tycon = match path.strip_suffix(".type") {
            Some(term) => {
                let (prefix, name) = term.rsplit_once('.').unwrap();
                let prefix = path_prefix(u, prefix);
                let name = u.intern_name(name);
                u.intern_type(Type::TermRef(prefix, name))
            }
            None => named_type(u, path),
        };
        if !self.eat("[") {
            return tycon;
        }
        let mut args = vec![self.infix(u)];
        while self.eat(", ") {
            args.push(self.infix(u));
        }
        assert!(self.eat("]"), "`]` expected at {}", self.s);
        u.intern_applied(tycon, &args)
    }
}

/// The type named by qualified name `path`: a class or type alias of a package, a reference by
/// name if the universe lacks it; a type member of a value's path.
pub fn named_type(u: &mut Universe, path: &str) -> TypeId {
    let (prefix, name) = path.rsplit_once('.').unwrap();
    let Some(pkg) = u.package(prefix) else {
        let prefix = path_prefix(u, prefix);
        let name = u.intern_name(name);
        return u.intern_type(Type::TypeRef(prefix, name));
    };
    let found = u
        .decl(pkg, name)
        .unwrap()
        .into_iter()
        .find(|&s| matches!(u.symbol(s).kind, Kind::Class | Kind::Type));
    match found {
        Some(s) => u.intern_type(Type::Sym(None, s)),
        None => u.class_ref(path),
    }
}

/// The path `path`: its longest package prefix, then values selected from it.
pub fn path_prefix(u: &mut Universe, path: &str) -> TypeId {
    let segments: Vec<&str> = path.split('.').collect();
    let packages =
        (0..=segments.len()).rev().find(|&i| u.package(&segments[..i].join(".")).is_some()).unwrap();
    let pkg = u.package(&segments[..packages].join(".")).unwrap();
    let mut t = u.intern_type(Type::Package(pkg));
    for term in &segments[packages..] {
        let term = u.intern_name(term);
        t = u.intern_type(Type::TermRef(t, term));
    }
    t
}

pub fn parse(u: &mut Universe, s: &str) -> TypeId {
    let mut p = Parser { s };
    let t = p.infix(u);
    assert!(p.s.is_empty(), "unparsed rest `{}` of `{s}`", p.s);
    t
}

/// Defines symbol `name` of kind `kind` in `owner`, of type `info`.
pub fn define(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId, info: TypeId) -> SymbolId {
    let flags = Flags::default();
    let (target, tree, decls) = (None, None, Vec::new());
    let s = u.define(Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags,
        target,
        is_extern: false,
        derived: false,
        decls,
        tree,
    });
    u.set_info(s, info);
    s
}
