//! Subtyping against dotc 3.9.0 (`just subtype-oracle`): the pairs of types in
//! `fixtures/subtype-types.txt`, decided by a bounds check, with the definitions of
//! `fixtures/subtype/Prelude.scala`.

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
                let (prefix, name) = term.rsplit_once('.').unwrap();
                let prefix = path_prefix(u, prefix);
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

/// The type named by qualified name `path`: a class or type alias of a package, a reference by
/// name if the universe lacks it; a type member of a value's path.
fn named_type(u: &mut Universe, path: &str) -> TypeId {
    let (prefix, name) = path.rsplit_once('.').unwrap();
    let Some(pkg) = u.package(prefix) else {
        let prefix = path_prefix(u, prefix);
        return u.intern_type(Type::TypeRef(prefix, name.into()));
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
fn path_prefix(u: &mut Universe, path: &str) -> TypeId {
    let segments: Vec<&str> = path.split('.').collect();
    let packages =
        (0..=segments.len()).rev().find(|&i| u.package(&segments[..i].join(".")).is_some()).unwrap();
    let pkg = u.package(&segments[..packages].join(".")).unwrap();
    let mut t = u.intern_type(Type::Package(pkg));
    for term in &segments[packages..] {
        t = u.intern_type(Type::TermRef(t, (*term).into()));
    }
    t
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

#[test]
fn members_of_types() {
    let Some(mut u) = universe() else { return };
    let has = |u: &mut Universe, t: &str, name: &str| {
        let t = parse(u, t);
        !u.member_of_type(t, name).is_empty()
    };
    assert!(has(&mut u, "oracle.a.v.type", "hashCode"));
    assert!(has(&mut u, "\"a\"", "length"));
    assert!(has(&mut u, "1", "+"));
    assert!(has(&mut u, "scala.Option[scala.Int] & scala.Product", "isEmpty"));
    assert!(has(&mut u, "scala.Option[scala.Int] & scala.Product", "productArity"));
    let union = "scala.collection.immutable.List[scala.Int] | scala.collection.immutable.Vector[scala.Int]";
    assert!(has(&mut u, union, "head"));
    assert!(!has(&mut u, union, "::"));
    assert!(has(&mut u, "oracle.Impl", "f"));
    assert!(!has(&mut u, "oracle.Impl", "g"));
}

#[test]
fn lub_and_glb_are_least_and_greatest_among_the_fixture_types() {
    let Some(mut u) = universe() else { return };
    let names: Vec<&str> =
        include_str!("fixtures/subtype-types.txt").lines().filter(|l| !l.is_empty()).collect();
    let types: Vec<TypeId> = names.iter().map(|n| parse(&mut u, n)).collect();
    let n = types.len();
    let sub: Vec<Vec<bool>> =
        (0..n).map(|i| (0..n).map(|j| u.is_subtype(types[i], types[j])).collect()).collect();
    let mut wrong = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (a, b) = (types[i], types[j]);
            let (lub, glb) = (u.lub(a, b), u.glb(a, b));
            let pair = format!("{} and {}", names[i], names[j]);
            if !u.is_subtype(a, lub) || !u.is_subtype(b, lub) {
                wrong.push(format!("lub of {pair}, {}, is not above both", u.show(lub)));
            }
            if !u.is_subtype(glb, a) || !u.is_subtype(glb, b) {
                wrong.push(format!("glb of {pair}, {}, is not below both", u.show(glb)));
            }
            for k in 0..n {
                if sub[i][k] && sub[j][k] && !u.is_subtype(lub, types[k]) {
                    wrong.push(format!("lub of {pair}, {}, is not below {}", u.show(lub), names[k]));
                }
                if sub[k][i] && sub[k][j] && !u.is_subtype(types[k], glb) {
                    wrong.push(format!("glb of {pair}, {}, is not above {}", u.show(glb), names[k]));
                }
            }
        }
    }
    let shown: Vec<&String> = wrong.iter().take(40).collect();
    assert!(wrong.is_empty(), "{} failures:\n{shown:#?}", wrong.len());
}
