use std::path::Path;
use tasty::{Binder, FileId, Flags, Kind, Symbol, SymbolId, Type, TypeId, Universe};

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
    let parent = u.intern_type(Type::Applied(option_t, vec![int_t]));
    u.set_parents(shape, vec![parent]);

    let get = u.define(symbol("get", Kind::Def, shape));
    u.set_info(get, int_t);
    let apply = u.define(symbol("apply", Kind::Def, shape));
    let x = u.define(symbol("x", Kind::Param, apply));
    u.set_info(x, int_t);
    let info = u.intern_type(Type::Method {
        binder: Binder(FileId(u32::MAX), 0),
        params: vec![("x".into(), int_t)],
        result: int_t,
        implicit: false,
        contextual: false,
    });
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
