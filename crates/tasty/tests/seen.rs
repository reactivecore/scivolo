use std::path::Path;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

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

fn applied(u: &mut Universe, c: SymbolId, args: &[SymbolId]) -> TypeId {
    let tycon = u.intern_type(Type::Sym(None, c));
    let args = args.iter().map(|&a| u.intern_type(Type::Sym(None, a))).collect();
    u.intern_type(Type::Applied(tycon, args))
}

#[test]
fn base_types_pass_arguments_through_parents() {
    let Some(mut u) = stdlib() else { return };
    let (list, int) = (class(&mut u, "scala.collection.immutable", "List"), class(&mut u, "scala", "Int"));
    let list_int = applied(&mut u, list, &[int]);
    let seq = class(&mut u, "scala.collection", "Seq");
    let base = u.base_type(list_int, seq).unwrap();
    assert_eq!(u.show(base), "scala.collection.Seq[Int]");
    let function1 = class(&mut u, "scala", "Function1");
    let base = u.base_type(list_int, function1).unwrap();
    assert_eq!(u.show(base), "scala.Function1[scala.Int, Int]");
    let option = class(&mut u, "scala", "Option");
    assert!(u.base_type(list_int, option).is_none());

    let (map, long) = (class(&mut u, "scala.collection.immutable", "Map"), class(&mut u, "scala", "Long"));
    let map_type = applied(&mut u, map, &[long, int]);
    let iterable = class(&mut u, "scala.collection", "Iterable");
    let base = u.base_type(map_type, iterable).unwrap();
    assert_eq!(u.show(base), "scala.collection.Iterable[scala.Tuple2[Long, Int]]");
}

fn this_type(u: &mut Universe, c: SymbolId) -> TypeId {
    let c = u.intern_type(Type::Sym(None, c));
    u.intern_type(Type::This(c))
}

#[test]
fn overriding_methods_match_and_overloads_do_not() {
    let Some(mut u) = stdlib() else { return };
    let lazy_list = class(&mut u, "scala.collection.immutable", "LazyList");
    let ops = class(&mut u, "scala.collection", "IterableOps");
    let site = this_type(&mut u, lazy_list);
    let (a, b) = (u.decl(lazy_list, "tapEach").unwrap()[0], u.decl(ops, "tapEach").unwrap()[0]);
    assert!(u.matches(site, a, b));

    let hash_map = class(&mut u, "scala.collection.immutable", "HashMap");
    let site = this_type(&mut u, hash_map);
    let own = u.decl(hash_map, "concat").unwrap()[0];
    let iterable_concat = u.decl(ops, "concat").unwrap()[0];
    assert!(!u.matches(site, own, iterable_concat));
    let map_ops = class(&mut u, "scala.collection", "MapOps");
    let map_concat = u.decl(map_ops, "concat").unwrap()[0];
    assert!(u.matches(site, own, map_concat));
}

#[test]
fn bridges_carry_the_overridden_signature() {
    let Some(mut u) = stdlib() else { return };
    let hash_map = class(&mut u, "scala.collection.immutable", "HashMap");
    let concat = u.decl(hash_map, "concat").unwrap()[0];
    let mut bridges = u.bridge_signatures(concat);
    bridges.sort();
    assert_eq!(
        bridges,
        [
            "D6concatL29scala.collection.IterableOnceL28scala.collection.IterableOpsEO",
            "D6concatL29scala.collection.IterableOnceL33scala.collection.immutable.MapOpsEO",
        ]
    );
}

#[test]
fn member_types_as_seen_from_a_prefix() {
    let Some(mut u) = stdlib() else { return };
    let (list, int) = (class(&mut u, "scala.collection.immutable", "List"), class(&mut u, "scala", "Int"));
    let list_int = applied(&mut u, list, &[int]);
    let head = u.member(list, "head")[0];
    let t = u.member_info(list_int, head);
    assert_eq!(u.show(t), "Int");
    let prepended = u.member(list, "::")[0];
    let t = u.member_info(list_int, prepended);
    assert!(u.show(t).contains("scala.collection.immutable.List[B]"), "{}", u.show(t));
}
