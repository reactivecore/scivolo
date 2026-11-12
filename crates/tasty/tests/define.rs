use std::path::Path;
use tasty::{Binder, Erased, FileId, Flags, Kind, Symbol, SymbolId, Type, TypeId, Universe};

fn stdlib() -> Option<Universe> {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    if !jar.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return None;
    }
    Some(Universe::open(&[jar]).unwrap())
}

fn class(u: &mut Universe, pkg: &str, name: &str) -> SymbolId {
    let p = u.package(pkg).unwrap();
    u.decl(p, name).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class).unwrap()
}

fn symbol(name: &str, kind: Kind, owner: SymbolId) -> Symbol {
    Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags: Flags::default(),
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    }
}

fn sym_type(u: &mut Universe, s: SymbolId) -> TypeId {
    u.intern_type(Type::Sym(None, s))
}

#[test]
fn script_classes_extend_library_classes() {
    let Some(mut u) = stdlib() else { return };
    let (option, int) = (class(&mut u, "scala", "Option"), class(&mut u, "scala", "Int"));
    let empty = u.enter_package("<empty>");
    assert_eq!(empty, u.root());
    let shape = u.define(symbol("Shape", Kind::Class, empty));
    let (option_t, int_t) = (sym_type(&mut u, option), sym_type(&mut u, int));
    let parent = u.intern_applied(option_t, &[int_t]);
    u.set_parents(shape, vec![parent]);

    let get = u.define(symbol("get", Kind::Def, shape));
    u.set_info(get, int_t);
    let apply = u.define(symbol("apply", Kind::Def, shape));
    let x = u.define(symbol("x", Kind::Param, apply));
    u.set_info(x, int_t);
    let x_name = u.intern_name("x");
    let info = u.intern_method(Binder(FileId(u32::MAX), 0), &[(x_name, int_t)], int_t, false, false);
    u.set_info(apply, info);

    assert_eq!(u.decl(empty, "Shape").unwrap(), [shape]);
    assert_eq!(u.member(shape, "get"), [get]);
    assert!(!u.member(shape, "isEmpty").is_empty());
    let iterable_once = class(&mut u, "scala.collection", "IterableOnce");
    assert!(u.derives_from(shape, iterable_once));
    let this = sym_type(&mut u, shape);
    let base = u.base_type(this, iterable_once).unwrap();
    assert_eq!(u.show(base), "scala.collection.IterableOnce[Int]");
    assert_eq!(u.method_signature(get).as_deref(), Some("D3getiEO"));
    assert_eq!(u.bridge_signatures(get), ["D3getL16java.lang.ObjectEO"]);
    assert_eq!(u.method_signature(apply).as_deref(), Some("D5applyiiEO"));
    let flags = Flags::default().with(tasty::tags::FINAL);
    u.set_flags(shape, flags);
    assert!(u.symbol(shape).flags.has(tasty::tags::FINAL));
}

#[test]
fn erased_methods_follow_changed_infos() {
    let mut u = Universe::empty();
    let root = u.root();
    let c = u.define(symbol("C", Kind::Class, root));
    let m = u.define(symbol("m", Kind::Def, c));
    let generation = u.generation();
    u.set_info(m, tasty::ANY);
    assert_eq!(u.generation(), generation);
    assert_eq!(u.erased_method(m).1, Erased::Class("java.lang.Object".into()));
    u.set_info(m, tasty::NOTHING);
    assert_ne!(u.generation(), generation);
    assert_eq!(u.erased_method(m).1, Erased::Nothing);
    let d = u.define(symbol("D", Kind::Class, root));
    let n = u.define(symbol("n", Kind::Def, c));
    let d_t = sym_type(&mut u, d);
    u.set_info(n, d_t);
    assert_eq!(u.erased_method(n).1, Erased::Class("D".into()));
    u.rename(d, "E");
    assert_eq!(u.erased_method(n).1, Erased::Class("E".into()));
    assert!(u.decl(c, "k").unwrap().is_empty());
    let generation = u.generation();
    u.define(symbol("k", Kind::Def, c));
    assert_ne!(u.generation(), generation);
}

#[test]
fn renamed_and_unentered_members_resolve_no_more() {
    let mut u = Universe::empty();
    let root = u.root();
    let c = u.define(symbol("C", Kind::Class, root));
    let c_t = sym_type(&mut u, c);
    let this = u.intern_type(Type::This(c_t));
    let name = u.intern_name("T");
    let r = u.intern_type(Type::TypeRef(this, name));
    let t = u.define(symbol("T", Kind::Type, c));
    assert_eq!(u.resolve_ref(r), Some(t));
    u.rename(t, "U");
    assert_eq!(u.resolve_ref(r), None);
    let t2 = u.define(symbol("T", Kind::Type, c));
    assert_eq!(u.resolve_ref(r), Some(t2));
    u.unenter(t2);
    assert_eq!(u.resolve_ref(r), None);
}

/// Checks `has_full_name` and `full_name_starts_with` of `s` against its `full_name`.
fn check_full_name(u: &Universe, s: SymbolId) {
    let full = u.full_name(s);
    assert!(u.has_full_name(s, &full), "{full}");
    assert!(!u.has_full_name(s, &format!("{full}x")), "{full}");
    assert!(!u.has_full_name(s, &format!("x{full}")), "{full}");
    assert!(!u.full_name_starts_with(s, &format!("{full}.")), "{full}");
    for i in full.char_indices().map(|(i, _)| i).chain([full.len()]) {
        let prefix = &full[..i];
        assert!(u.full_name_starts_with(s, prefix), "{full} / {prefix}");
        assert_eq!(u.has_full_name(s, prefix), i == full.len(), "{full} / {prefix}");
        assert!(!u.full_name_starts_with(s, &format!("{prefix}\u{1}")), "{full} / {prefix}");
    }
}

#[test]
fn full_name_tests_agree_with_full_names() {
    let mut u = Universe::empty();
    let root = u.root();
    let empty = u.define(symbol("<empty>", Kind::Package, root));
    let a = u.define(symbol("a", Kind::Package, root));
    let dotted = u.define(symbol("b.c", Kind::Class, a));
    let pkg_object = u.define(symbol("package$", Kind::Class, a));
    let outer = u.define(symbol("Outer$", Kind::Class, pkg_object));
    let inner = u.define(symbol("Inner$", Kind::Class, outer));
    let mut synthetic = vec![root, empty, a, dotted, pkg_object, outer, inner];
    for name in ["::", "a b", "x$y$", "", "$u0020", "a.b"] {
        synthetic.push(u.define(symbol(name, Kind::Class, inner)));
        synthetic.push(u.define(symbol(name, Kind::Def, empty)));
        synthetic.push(u.define(symbol(name, Kind::Val, dotted)));
    }
    synthetic.iter().for_each(|&s| check_full_name(&u, s));
    assert!(u.has_full_name(dotted, "a.b.c") && u.full_name_starts_with(dotted, "a.b"));

    let Some(mut u) = stdlib() else { return };
    for (pkg, name) in
        [("scala", "Predef$"), ("scala", "::"), ("scala", "package$"), ("scala.collection", "List")]
    {
        let p = u.package(pkg).unwrap();
        let _ = u.decl(p, name);
    }
    (0..u.next_symbol().0).for_each(|i| check_full_name(&u, SymbolId(i)));
}
