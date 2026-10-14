//! Subtyping against dotc 3.9.0 (`just subtype-oracle`): the pairs of types in
//! `fixtures/subtype-types.txt`, decided by a bounds check, with the definitions of
//! `fixtures/subtype/Prelude.scala`.

use std::path::Path;
use tasty::{ANY, Binder, FileId, Flags, Kind, NOTHING, Symbol, SymbolId, Type, TypeId, Universe};

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
fn unions_of_singletons_compare_by_atoms() {
    let Some(mut u) = universe() else { return };
    let dotc = [
        ("1", "1 | 2", true),
        ("3", "1 | 2", false),
        ("1 | 2", "1 | 2 | 3", true),
        ("1 | 2 | 3", "1 | 2", false),
        ("1 | 2", "2 | 1", true),
        ("1 | 2", "scala.Int", true),
        ("scala.Int", "1 | 2", false),
        ("scala.Nothing", "1 | 2", true),
        ("scala.Nothing | 1", "1 | 2", true),
        ("1", "scala.Int | java.lang.String", true),
        ("1 | 2", "1 | scala.Int", true),
        ("scala.Int | java.lang.String", "java.lang.String | scala.Int", true),
        ("\"a\" | 1", "\"a\" | 1 | 2", true),
        ("\"b\"", "\"a\" | 1", false),
        ("oracle.a.v.type", "oracle.a.v.type | 1", true),
        ("oracle.a.v.type", "oracle.b.v.type | 1", false),
        ("oracle.a.v.type | 1", "oracle.a.v.type | 1 | 2", true),
        ("oracle.a.type | oracle.b.type", "oracle.b.type | oracle.a.type", true),
        ("scala.Null", "oracle.a.v.type | 1", false),
        ("oracle.a.T", "1 | 2", false),
        ("oracle.O.type", "oracle.O.type | 1", true),
    ];
    for (a, b, expected) in dotc {
        let (ta, tb) = (parse(&mut u, a), parse(&mut u, b));
        assert_eq!(u.is_subtype(ta, tb), expected, "{a} <: {b}");
    }
}

#[test]
fn paths_to_one_value_are_one_atom() {
    let Some(mut u) = universe() else { return };
    let none_ref = parse(&mut u, "scala.None.type");
    let scala = path_prefix(&mut u, "scala");
    let none = u.resolve(scala, "None", false).unwrap();
    let none_sym = u.intern_type(Type::Sym(None, none));
    let oracle = path_prefix(&mut u, "oracle");
    let o = u.resolve(oracle, "O", false).unwrap();
    let o_class = u.module_class(o).unwrap();
    let o_class = u.intern_type(Type::Sym(None, o_class));
    let o_this = u.intern_type(Type::This(o_class));
    let pairs =
        [(none_sym, "scala.None.type | 1"), (o_this, "oracle.O.type | 1"), (o_this, "1 | oracle.O.type")];
    for (a, b) in pairs {
        let tb = parse(&mut u, b);
        assert!(u.is_subtype(a, tb), "{} <: {b}", u.show(a));
    }
    let one = parse(&mut u, "1");
    let union = u.intern_type(Type::Or(none_sym, one, false));
    assert!(u.is_subtype(none_ref, union));
    assert!(!u.is_subtype(o_this, union));
}

#[test]
fn module_classes_and_this_of_objects_have_the_object_as_atom() {
    let Some(mut u) = universe() else { return };
    let none_class = named_type(&mut u, "scala.None$");
    let none_or_1 = parse(&mut u, "scala.None.type | 1");
    assert!(u.is_subtype(none_class, none_or_1));
    let (root, template) = (u.root(), u.intern_type(Type::Other(tasty::tags::TEMPLATE)));
    let c = define(&mut u, "C", Kind::Class, root, template);
    let o_class = define(&mut u, "O$", Kind::Class, c, template);
    let o_class_ref = u.intern_type(Type::Sym(None, o_class));
    let o = define(&mut u, "O", Kind::Val, c, o_class_ref);
    let object = Flags::default().with(tasty::tags::OBJECT);
    u.set_flags(o_class, object);
    u.set_flags(o, object);
    let c_ref = u.intern_type(Type::Sym(None, c));
    let c_this = u.intern_type(Type::This(c_ref));
    let o_ref = u.intern_type(Type::Sym(Some(c_this), o));
    let (one, a_or_1) = (parse(&mut u, "1"), parse(&mut u, "oracle.a.type | 1"));
    let o_or_1 = u.intern_type(Type::Or(o_ref, one, false));
    let o_this = u.intern_type(Type::This(o_class_ref));
    assert!(u.is_subtype(o_this, o_or_1));
    assert!(!u.is_subtype(o_this, a_or_1));
}

#[test]
fn stable_methods_are_atoms() {
    let Some(mut u) = universe() else { return };
    let (root, int) = (u.root(), u.class_ref("scala.Int"));
    let d = define(&mut u, "d", Kind::Def, root, int);
    u.set_flags(d, Flags::default().with(tasty::tags::STABLE));
    u.enter(d);
    let (package, one) = (u.intern_type(Type::Package(root)), parse(&mut u, "1"));
    let d_ref = u.intern_type(Type::TermRef(package, "d".into()));
    let d_or_1 = u.intern_type(Type::Or(d_ref, one, false));
    let d_sym = u.intern_type(Type::Sym(None, d));
    assert!(u.is_subtype(d_sym, d_or_1));
}

#[test]
fn unions_with_unresolved_parts_compare_by_their_parts() {
    let Some(mut u) = universe() else { return };
    let missing = named_type(&mut u, "oracle.Missing");
    let one = parse(&mut u, "1");
    let missing_or_1 = u.intern_type(Type::Or(missing, one, false));
    assert!(u.is_subtype(missing, missing_or_1));
    let (zero, one_or_2) = (parse(&mut u, "0"), parse(&mut u, "1 | 2"));
    let missing_or_0 = u.intern_type(Type::Or(missing, zero, false));
    assert!(!u.is_subtype(missing_or_0, one_or_2));
}

/// A union of integer constants, its parts parenthesized.
fn shape(u: &Universe, t: TypeId) -> String {
    match *u.types().get(t) {
        Type::Or(x, y, _) => format!("({}|{})", shape(u, x), shape(u, y)),
        Type::Constant(_, n) => n.to_string(),
        _ => panic!("{} is not a union of constants", u.show(t)),
    }
}

#[test]
fn unions_of_constants_grow_balanced_as_in_dotc() {
    let Some(mut u) = universe() else { return };
    let dotc = [
        "1",
        "(1|2)",
        "((1|2)|3)",
        "((1|2)|(3|4))",
        "((1|2)|(3|(4|5)))",
        "((1|(2|3))|(4|(5|6)))",
        "((1|(2|3))|((4|5)|(6|7)))",
        "(((1|2)|(3|4))|((5|6)|(7|8)))",
        "(((1|2)|(3|4))|((5|6)|(7|(8|9))))",
    ];
    let (mut by_lub, mut by_atoms) = (NOTHING, NOTHING);
    for (i, expected) in dotc.into_iter().enumerate() {
        let c = parse(&mut u, &(i + 1).to_string());
        by_lub = u.lub(by_lub, c);
        by_atoms = if i == 0 { c } else { u.atoms_lub(by_atoms, c).unwrap() };
        assert_eq!(shape(&u, by_lub), expected);
        assert_eq!(by_atoms, by_lub, "{expected}");
    }
}

#[test]
fn soft_and_hard_unions_are_distinct_but_equivalent() {
    let Some(mut u) = universe() else { return };
    let (one, two) = (parse(&mut u, "1"), parse(&mut u, "2"));
    let (soft, hard) = (u.lub(one, two), u.lub_as(one, two, false));
    assert_ne!(soft, hard);
    assert!(matches!(u.types().get(soft), Type::Or(_, _, true)));
    assert!(matches!(u.types().get(hard), Type::Or(_, _, false)));
    assert!(u.is_subtype(soft, hard) && u.is_subtype(hard, soft));
}

#[test]
fn balancing_keeps_a_union_of_the_other_softness_whole() {
    let Some(mut u) = universe() else { return };
    let [one, two, three, four] = ["1", "2", "3", "4"].map(|s| parse(&mut u, s));
    let hard = u.lub_as(one, two, false);
    let hard = u.lub_as(hard, three, false);
    let kept = u.union(hard, four, true);
    assert_eq!(*u.types().get(kept), Type::Or(hard, four, true));
    let soft = u.lub(one, two);
    let soft = u.lub(soft, three);
    let balanced = u.union(soft, four, true);
    assert_eq!(shape(&u, balanced), "((1|2)|(3|4))");
}

fn define(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId, info: TypeId) -> SymbolId {
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

#[test]
fn bounds_locals_and_this_as_atoms() {
    let Some(mut u) = universe() else { return };
    let (root, int, one) = (u.root(), parse(&mut u, "scala.Int"), parse(&mut u, "1"));
    let sym = |u: &mut Universe, prefix, s| u.intern_type(Type::Sym(prefix, s));
    let or = |u: &mut Universe, a, b| u.intern_type(Type::Or(a, b, false));
    let a = parse(&mut u, "oracle.a.type");
    let below_a = u.intern_type(Type::Bounds(NOTHING, a));
    let t = define(&mut u, "T", Kind::Type, root, below_a);
    let t = sym(&mut u, None, t);
    let (any_bounds, below_one) =
        (u.intern_type(Type::Bounds(NOTHING, ANY)), u.intern_type(Type::Bounds(NOTHING, one)));
    let binder = Binder(FileId(u32::MAX), 0);
    let lambda =
        u.intern_type(Type::Lambda { binder, params: vec![("X".into(), any_bounds)], result: below_one });
    let f = define(&mut u, "F", Kind::Type, root, lambda);
    let f = sym(&mut u, None, f);
    let f_int = u.intern_type(Type::Applied(f, vec![int]));
    let m = define(&mut u, "m", Kind::Def, root, int);
    let (p, q) = (define(&mut u, "p", Kind::Param, m, int), define(&mut u, "q", Kind::Param, m, int));
    let (p, q) = (sym(&mut u, None, p), sym(&mut u, None, q));
    let a_or_1 = or(&mut u, a, one);
    let (q_or_1, p_or_1) = (or(&mut u, q, one), or(&mut u, p, one));
    let two = parse(&mut u, "2");
    let (f_int_or_2, one_or_2) = (or(&mut u, f_int, two), or(&mut u, one, two));
    let boxed = named_type(&mut u, "oracle.Box");
    let box_this = u.intern_type(Type::This(boxed));
    let v = u.member_of_type(boxed, "v")[0];
    let (this_v, v) = (sym(&mut u, Some(box_this), v), sym(&mut u, None, v));
    let v_or_1 = or(&mut u, v, one);
    assert!(!u.is_subtype(a, t));
    assert!(u.is_subtype(t, a_or_1));
    assert!(u.is_subtype(f_int_or_2, one_or_2));
    assert!(!u.is_subtype(p, q_or_1));
    assert!(u.is_subtype(p, p_or_1));
    assert!(!u.is_subtype(box_this, a_or_1));
    assert!(u.is_subtype(this_v, v_or_1));
}

#[test]
fn union_parts_with_class_symbol_nothing_have_no_atoms() {
    let Some(mut u) = universe() else { return };
    let root = u.root();
    let (one, one_or_2) = (parse(&mut u, "1"), parse(&mut u, "1 | 2"));
    let sym = |u: &mut Universe, s| u.intern_type(Type::Sym(None, s));
    let or_1 = |u: &mut Universe, a| u.intern_type(Type::Or(a, one, false));
    let (below_nothing, unbounded) =
        (u.intern_type(Type::Bounds(NOTHING, NOTHING)), u.intern_type(Type::Bounds(NOTHING, ANY)));
    let (s, x) = (
        define(&mut u, "S", Kind::Type, root, below_nothing),
        define(&mut u, "X", Kind::Type, root, unbounded),
    );
    let (s, x) = (sym(&mut u, s), sym(&mut u, x));
    let below_s = u.intern_type(Type::Bounds(NOTHING, s));
    let t = define(&mut u, "T", Kind::Type, root, below_s);
    let t = sym(&mut u, t);
    let n = define(&mut u, "n", Kind::Val, root, NOTHING);
    let n = sym(&mut u, n);
    let nothing_and_x = u.intern_type(Type::And(NOTHING, x));
    for part in [nothing_and_x, t, n] {
        let union = or_1(&mut u, part);
        assert!(u.is_subtype(union, one_or_2), "{} <: 1 | 2", u.show(union));
    }
    let x_or_1 = or_1(&mut u, x);
    assert!(!u.is_subtype(x_or_1, one_or_2));
}

/// Abstract types `T0 <: 2` and `Tk <: (Tk-1 | 1) & (Tk-1 | 3)`, each with atoms `2`.
fn bound_dag(u: &mut Universe, levels: usize) -> TypeId {
    let root = u.root();
    let (one, two, three) = (parse(u, "1"), parse(u, "2"), parse(u, "3"));
    let mut bound = two;
    let mut t = NOTHING;
    for k in 0..=levels {
        let bounds = u.intern_type(Type::Bounds(NOTHING, bound));
        let s = define(u, &format!("T{k}"), Kind::Type, root, bounds);
        t = u.intern_type(Type::Sym(None, s));
        let (left, right) =
            (u.intern_type(Type::Or(t, one, false)), u.intern_type(Type::Or(t, three, false)));
        bound = u.intern_type(Type::And(left, right));
    }
    t
}

#[test]
fn atoms_of_a_dag_of_bounds_take_one_step_per_node() {
    let Some(mut u) = universe() else { return };
    let two_or_4 = parse(&mut u, "2 | 4");
    let t = bound_dag(&mut u, 40);
    assert!(u.is_subtype(t, two_or_4));
}

#[test]
fn comparisons_of_a_dag_of_bounds_take_one_step_per_pair() {
    let Some(mut u) = universe() else { return };
    let one_or_3 = parse(&mut u, "1 | 3");
    let t = bound_dag(&mut u, 40);
    let start = u.subtype_steps;
    assert!(!u.is_subtype(t, one_or_3));
    let (one, three, string) = (parse(&mut u, "1"), parse(&mut u, "3"), u.class_ref("java.lang.String"));
    assert!(!u.is_subtype(t, one));
    assert!(!u.is_subtype(t, three));
    assert!(!u.is_subtype(t, string));
    let one_or_t = u.intern_type(Type::Or(one, t, false));
    assert!(!u.is_subtype(one_or_t, three));
    u.lub(t, one);
    u.glb(t, one_or_3);
    let steps = u.subtype_steps - start;
    assert!(steps < 1_000_000, "took {steps} steps");
}

#[test]
fn outer_this_types_are_seen_from_the_prefix() {
    let Some(mut u) = universe() else { return };
    let outer = named_type(&mut u, "oracle.Outer");
    let outer_class = u.class_symbol(outer).unwrap();
    let decls = u.decl(outer_class, "Inner").unwrap();
    let inner = decls.into_iter().find(|&s| u.symbol(s).kind == Kind::Class).unwrap();
    let this = u.intern_type(Type::This(outer));
    let in_outer = u.intern_type(Type::Sym(Some(this), inner));
    let outer1_inner = parse(&mut u, "oracle.outer1.Inner");
    let seen = u.as_seen_from(in_outer, outer1_inner, inner);
    let outer1 = path_prefix(&mut u, "oracle.outer1");
    assert_eq!(u.types().get(seen), &Type::Sym(Some(outer1), inner));
}

#[test]
fn any_declares_the_universal_members_and_object_adds_eq() {
    let Some(mut u) = universe() else { return };
    let object = parse(&mut u, "java.lang.Object");
    let owners = |u: &mut Universe, name: &str| -> Vec<String> {
        let members = u.member_of_type(object, name);
        members.into_iter().map(|m| u.full_name(u.symbol(m).owner.unwrap())).collect()
    };
    assert_eq!(owners(&mut u, "toString"), ["scala.Any"]);
    assert_eq!(owners(&mut u, "=="), ["scala.Any"]);
    assert_eq!(owners(&mut u, "eq"), ["java.lang.Object"]);
    assert_eq!(owners(&mut u, "synchronized"), ["java.lang.Object"]);
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
    let nested = format!("{union} | scala.collection.immutable.Nil.type");
    assert!(has(&mut u, &nested, "head"));
    assert!(has(&mut u, "oracle.AB", "f"));
    assert!(has(&mut u, "oracle.AB", "v"));
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
                // A union glb is not checked against `Null`.
                let null = names[k] == "scala.Null" && matches!(u.types().get(glb), Type::Or(..));
                if sub[k][i] && sub[k][j] && !null && !u.is_subtype(types[k], glb) {
                    wrong.push(format!("glb of {pair}, {}, is not above {}", u.show(glb), names[k]));
                }
            }
        }
    }
    let shown: Vec<&String> = wrong.iter().take(40).collect();
    assert!(wrong.is_empty(), "{} failures:\n{shown:#?}", wrong.len());
}
