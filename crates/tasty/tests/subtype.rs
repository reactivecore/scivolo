//! Subtyping against dotc 3.9.0 (`just subtype-oracle`): the pairs of types in
//! `fixtures/subtype-types.txt`, decided by `summon[A <:< B]`.

use std::path::Path;
use tasty::{ANY, Kind, NOTHING, Type, TypeId, Universe};

fn universe() -> Option<Universe> {
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
                t = u.intern_type(Type::Or(t, r));
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
            return u.intern_type(Type::Str(text.into()));
        }
        let end = self.s.find(['[', ',', ']', ' ']).unwrap_or(self.s.len());
        let (path, rest) = self.s.split_at(end);
        self.s = rest;
        if let Ok(n) = path.parse::<i64>() {
            return u.intern_type(Type::Constant(tasty::tags::INTconst, n));
        }
        let tycon = match path.strip_suffix(".type") {
            Some(term) => {
                let (pkg, name) = term.rsplit_once('.').unwrap();
                let prefix = u.package(pkg).unwrap();
                let prefix = u.intern_type(Type::Package(prefix));
                u.intern_type(Type::TermRef(prefix, name.into()))
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
        u.intern_type(Type::Applied(tycon, args))
    }
}

/// The class or type alias of qualified name `path`, a reference by name if the universe
/// lacks it.
fn named_type(u: &mut Universe, path: &str) -> TypeId {
    let (pkg, name) = path.rsplit_once('.').unwrap();
    let pkg = u.package(pkg).unwrap();
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

fn parse(u: &mut Universe, s: &str) -> TypeId {
    let mut p = Parser { s };
    let t = p.infix(u);
    assert!(p.s.is_empty(), "unparsed rest `{}` of `{s}`", p.s);
    t
}

#[test]
fn subtyping_matches_dotc() {
    let Some(mut u) = universe() else { return };
    let oracle = include_str!("fixtures/subtype-oracle.tsv");
    let (mut total, mut wrong) = (0, Vec::new());
    for line in oracle.lines() {
        let [a, b, expected] = line.split('\t').collect::<Vec<_>>()[..] else { panic!("bad line {line}") };
        let (ta, tb) = (parse(&mut u, a), parse(&mut u, b));
        let ours = u.is_subtype(ta, tb);
        total += 1;
        if ours.to_string() != expected {
            wrong.push(format!("{a} <: {b}: dotc {expected}, ours {ours}"));
        }
    }
    eprintln!("{} of {total} pairs differ\n{}", wrong.len(), wrong.join("\n"));
    assert!(wrong.is_empty(), "{} of {total} pairs differ", wrong.len());
}
